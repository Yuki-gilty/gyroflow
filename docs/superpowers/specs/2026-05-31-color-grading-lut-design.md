# カラーグレーディング & .cube LUT 対応 — 設計仕様

- 日付: 2026-05-31
- 対象: Gyroflow (Rust core + Qt/QML UI)
- ステータス: 承認済み（実装プラン作成へ）

## 1. 目的

Gyroflow に Premiere Pro の Lumetri カラー風のカラーグレーディング機能を追加する。具体的には:

1. `.cube` 形式の LUT ファイル（1D / 3D）を適用できるようにする。
2. 露光量・コントラスト・ハイライト等の色調整 UI を、右側の設定パネルに新しい折りたたみセクションとして追加する。

調整結果はプレビュー表示と書き出し動画の**両方**に反映される（同一レンダリングパイプラインのため）。

## 2. スコープ（合意事項）

含む:
- 適用範囲: プレビュー + 書き出しの両方（段階的: プレビュー先行 → 書き出し対応。書き出し対応は必須）
- バックエンド: プレビュー = qt_gpu（GLSL）、書き出し = wgpu + CPU フォールバック（OpenCL は将来対応として未変更でバイパス）
- 調整項目: フルセット + クリエイティブ
- LUT: **2 つのスロット**（基本補正 = 入力 LUT、クリエイティブ = ルック LUT）。`.cube` の 1D / 3D 両対応
- キーフレーム: なし。クリップ全体で固定値

含まない（今回見送り）:
- 自動補正ボタン
- ホワイトバランスのスポイト（色拾い）
- シャープ（近傍ピクセル参照が必要で現パイプラインと相性が悪い）
- キーフレーム
- 色空間情報ドロップダウン

## 3. アーキテクチャ概要

### 3.1 調査で判明した重要な制約（設計改訂）

プラン作成時の詳細調査により、当初想定と異なる以下の事実が判明した。これにより設計を改訂する。

1. **プレビューと書き出しで別のシェーダ実装を使う。**
   - プレビュー（画面表示）: `src/qt_gpu/undistort.frag`（Qt RHI / GLSL, QSB にコンパイル）。入力は **常に RGBA8（1 枚のインターリーブテクスチャ, binding 0 = `inputTexture`）**。
   - 書き出し: `src/core/gpu/wgpu_undistort.wgsl`（wgpu）/ OpenCL / CPU。出力ピクセル形式に依存。
2. **書き出しの YUV 系形式（NV12 / P010 / YUV420P / YUV444P 等）はプレーンごとに別実行**される（`src/rendering/mod.rs` がプレーン単位で `process_pixels` を呼ぶ。`KernelParams.plane_index` で識別）。1 回のシェーダ実行は単一プレーン（Y のみ、UV のみ等）しか参照できないため、**RGB を混合する色処理（ホワイトバランス・彩度・LUT 等）を undistort 内に直接入れても YUV 書き出しでは正しく計算できない**。

### 3.2 改訂後のアーキテクチャ

カラーグレーディングは **undistort とは独立した「フル RGBA フレームに対する per-pixel 処理」** として実装し、プレビューと書き出しの両方の経路にそれを通す。色処理の数式は 1 か所で設計し、GLSL（プレビュー）と WGSL/CPU（書き出し）に同じ式を展開する。

```
プレビュー経路:
  decoded frame → qt_gpu undistort.frag (RGBA8) → [NEW] color grading (RGBA) → 画面

書き出し経路:
  decoded frame → wgpu/CPU undistort (per-plane) → [NEW] color grading pass (full RGBA; YUV は RGB 変換を挟む) → encoder
```

### 3.3 段階的方針（合意）

ユーザー合意により **プレビューを先に完成させ、最終的に書き出しも対応する**。

- フェーズ A（本プラン優先）: プレビュー（qt_gpu）でのカラーグレーディング + LUT を完成。画面で調整・確認できる状態にする。
- フェーズ B（後続）: 同じ数式を wgpu/CPU 書き出し経路に展開。YUV 形式は RGB 変換を挟んで適用。**書き出しも必ず対応する**こと（ユーザー明示の要件）。

色処理の数式とパラメータ構造（`color_grading.rs` / `lut.rs`）はフェーズ A・B で共有するため、フェーズ A の時点で書き出しに再利用できる形に設計する。

## 4. コンポーネント構成（責務分離）

| モジュール | 場所（新規/既存） | 責務 | 依存 |
|---|---|---|---|
| `.cube` パーサ | 新規 `src/core/lut.rs` | Adobe `.cube` (1D/3D) をパース。`LUT_1D_SIZE` / `LUT_3D_SIZE` / `DOMAIN_MIN` / `DOMAIN_MAX` / テーブルを読み、GPU 3D テクスチャ用に正規化したデータを生成（1D は 3D へ昇格）。純 Rust・単体テスト可能 | なし（標準ライブラリのみ） |
| カラーパラメータ | 新規 `src/core/color_grading.rs` | 全スライダー値 + 2 つの LUT スロット（パース済みデータ + 元パス + 強度 + 有効フラグ）を保持する `ColorGradingParams` 構造体。`serde` 対応 | `lut.rs` |
| GPU 統合 | `src/core/stabilization/mod.rs` (`KernelParams`), `src/core/gpu/wgpu.rs`, `src/core/gpu/wgpu_undistort.wgsl` | `KernelParams` にスカラー値 + フラグを追加（16 byte アライン維持）。LUT を 3D テクスチャ + サンプラとして bind。シェーダ末尾に色処理関数群を追加 | `color_grading.rs` |
| CPU フォールバック | `src/core/stabilization/cpu_undistort.rs` | 同じ色数式を Rust で実装（GPU 無し環境用）。シェーダと数式を一致させる | `color_grading.rs` |
| コントローラ橋渡し | `src/controller.rs` | 各スカラーの setter（`wrap_simple_method!` + recompute）、`set_lut_file(slot, path)`、セクション有効トグル、リセット | core API |
| QML UI | 新規 `src/ui/menu/ColorGrading.qml` + `src/ui/App.qml` 登録 | 「基本補正」「クリエイティブ」2 つの `MenuItem` セクション | 既存コンポーネント |

各ユニットの境界:
- `lut.rs`: 入力 = ファイルパス/文字列、出力 = 正規化済み LUT データ。GPU や UI を知らない。
- `color_grading.rs`: 値の保持とシリアライズのみ。レンダリングを知らない。
- GPU / CPU 実装: `ColorGradingParams` を読み、ピクセルに数式を適用する。

## 5. 色処理パイプライン順（per-pixel）

**基本補正**（トグル ON 時）:
1. 入力 LUT（スロット 1）を強度でブレンド
2. ホワイトバランス（色温度・色かぶり補正/tint）
3. 露光量
4. コントラスト
5. ハイライト / シャドウ / 白レベル / 黒レベル（トーン）
6. 彩度

**クリエイティブ**（トグル ON 時）:
7. ルック LUT（スロット 2）を強度でブレンド
8. 色あせたフィルム
9. 自然な彩度（vibrance）
10. 彩度

恒等条件（全項目デフォルト + 恒等 LUT もしくは LUT 未設定）のとき、出力 = 入力（無変化）であることをテストで保証する。

## 6. UI 項目（右パネル）

**基本補正** ［有効トグル］
- 入力 LUT（ファイル選択, `.cube`）+ 強度 0–100（既定 100）
- カラー: 色温度 −100..100 / 色かぶり補正 −100..100 / 彩度 0..200（既定 100）
- ライト: 露光量 / コントラスト / ハイライト / シャドウ / 白レベル / 黒レベル（各 −100..100, 既定 0）
- リセット

**クリエイティブ** ［有効トグル］
- ルック（ファイル選択, `.cube`）+ 強さ 0–100（既定 100）
- 調整: 色あせたフィルム 0..100（既定 0）/ 自然な彩度 −100..100（既定 0）/ 彩度 0..200（既定 100）

既存の `MenuItem` / `SliderWithField` / `ComboBox` / `FileDialog` を再利用する。App.qml の SidePanel に既存セクション（Stabilization など）と同じパターンで `ItemLoader` + `Hr` を追加登録する。

## 7. データフロー

```
QML スライダー
  → controller.set_x(v)
  → manager.params.write().color_grading.x = v
  → request_recompute()
  → recompute_threaded()
  → process_pixels() が KernelParams を構築（色フィールド設定 + LUT テクスチャ bind）
  → シェーダ / CPU が色処理を適用
```

LUT ファイル選択時:
```
QML FileDialog
  → controller.set_lut_file(slot, path)
  → lut.rs でパース
  → ColorGradingParams のスロットに格納
  → request_recompute()
```

## 8. 永続化

`.gyroflow` プロジェクトファイルの既存 `serde` シリアライズに `color_grading` フィールドを追加する。LUT はファイルパス参照として保存し、再読込時にパスから再ロードする（ファイルが見つからない場合は無効化し警告）。`ColorGradingParams::default()` は全項目恒等（無変化）とし、既存プロジェクトとの後方互換を保つ。

## 9. テスト戦略

- `.cube` パーサ単体テスト: 1D / 3D / `DOMAIN_MIN`/`MAX` / 不正フォーマット / 空ファイル
- 色数式の CPU 参照実装テスト: 露光・コントラスト・彩度・WB・恒等 LUT の既知入出力
- 恒等テスト: 全デフォルト値で入力 = 出力
- CPU と GPU（wgpu）の数式一致は、同一入力に対する近似一致で確認（許容誤差付き）

## 10. 実装時に詰める細部（リスク）

- `KernelParams` の 16 byte アライン: フィールド追加時にコメントのバイト計算（mod.rs:101-148）と `opencl_undistort.cl` / `qt_gpu/undistort.frag` との同期に注意（OpenCL/qt_gpu は色処理は未実装でも構造体レイアウトは一致させる）。
- wgpu の既存 bind group への 3D LUT テクスチャ + サンプラ追加（binding index の割当、レイアウト変更）。
- 作業色空間（sRGB か linear か）: まずは映像の既存レンジ上でシンプルに行い、選択を実装内コメントに文書化する。
- macOS のプレビュー経路が wgpu であることを実装時に確認（mdk/qt_gpu 経路使用時の扱い）。

## 11. 段階的実装の想定（プランで詳細化）

1. `.cube` パーサ + 単体テスト
2. `ColorGradingParams` + serde + デフォルト恒等
3. CPU 参照実装（色数式）+ テスト
4. wgpu シェーダ統合（スカラー）
5. wgpu LUT テクスチャ統合
6. controller 橋渡し
7. QML UI（基本補正 + クリエイティブ）
8. 永続化・統合確認
