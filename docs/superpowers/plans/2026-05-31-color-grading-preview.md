# Color Grading (Preview) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Premiere-Pro-style color grading panel (exposure, contrast, tone, white balance, saturation, vibrance, faded film) to Gyroflow's right-side settings panel, applied live in the on-screen preview.

**Architecture:** Color grading is a per-pixel operation applied at the END of the preview undistort fragment shader (`src/qt_gpu/undistort.frag`), where the frame is already interleaved RGBA8. Scalar parameters are appended to the existing `KernelParams` uniform (no new GPU bindings), plumbed QML → controller → core → shader. LUT (`.cube`) loading and export-path baking are deliberately split into follow-on plans (see end).

**Tech Stack:** Rust (gyroflow-core), Qt/QML (qmetaobject), GLSL (Qt RHI / QSB), serde_json.

**Scope of THIS plan (Phase A, no LUT):** the scalar sliders only — 基本補正 (色温度/色かぶり/彩度/露光量/コントラスト/ハイライト/シャドウ/白レベル/黒レベル) and クリエイティブ (色あせたフィルム/自然な彩度/彩度), with section enable toggles and reset. LUT slots appear in a later plan.

**Why scalars-only first:** Scalar fields fit in the existing `KernelParams` uniform buffer that is already uploaded to the preview shader — no new texture/sampler bindings or Qt RHI C++ changes are required. This produces a working, testable, visible feature. LUT needs a new 3D texture binding in `src/qt_gpu/qrhi_undistort.cpp` (Qt RHI) and is handled separately.

---

## File Structure

| File | New/Modify | Responsibility |
|------|-----------|----------------|
| `src/core/color_grading.rs` | Create | `ColorGradingParams` struct: all scalar values + section toggles. serde. `default()` = identity (no change). Pure data. |
| `src/core/lib.rs` | Modify | `mod color_grading;` + re-export; setter methods on `StabilizationManager` writing into `params.color_grading`; serialize/deserialize in `export_gyroflow_data` / import. |
| `src/core/stabilization_params.rs` | Modify | Add `pub color_grading: crate::color_grading::ColorGradingParams` field + default. |
| `src/core/stabilization/mod.rs` | Modify | Append color-grading scalar fields to `KernelParams` (kept 16-byte aligned). |
| `src/core/stabilization/frame_transform.rs` | Modify | Populate the new `KernelParams` color fields from `ComputeParams`/params when building a `FrameTransform`. |
| `src/core/stabilization/compute_params.rs` | Modify | Carry `color_grading` from `StabilizationParams` into `ComputeParams`. |
| `src/qt_gpu/undistort.frag` | Modify | Mirror the new uniform fields in the `buf` block; add GLSL color-grading functions; apply to final `fragColor` (preview). |
| `src/core/gpu/wgpu_undistort.wgsl` | Modify | Mirror the new fields in WGSL `KernelParams` struct (layout sync only; not applied yet). |
| `src/core/gpu/opencl_undistort.cl` | Modify | Mirror the new fields in the OpenCL `KernelParams` struct (layout sync only). |
| `src/controller.rs` | Modify | `qt_method!` declarations + `wrap_simple_method!` setters; reset method; section-toggle methods. |
| `src/ui/menu/ColorGrading.qml` | Create | The two collapsible sections (基本補正 / クリエイティブ) with sliders. |
| `src/ui/menu/qmldir` | Modify | Register `ColorGrading 1.0 ColorGrading.qml`. |
| `src/ui/App.qml` | Modify | Add `ItemLoader { ... Menu.ColorGrading { } }` + `Hr` in the right SidePanel. |

**Key invariant:** `KernelParams` is mirrored across 4 places — `src/core/stabilization/mod.rs` (Rust, source of truth), `src/core/gpu/wgpu_undistort.wgsl`, `src/qt_gpu/undistort.frag`, `src/core/gpu/opencl_undistort.cl`. Fields are **appended at the end** so existing offsets are untouched. All four must add the same fields in the same order or the GPU buffer layout breaks.

---

## Field Layout (append to end of KernelParams)

The current `KernelParams` ends with:
```rust
pub ewa_coeffs_p: [f32; 4], // 16
pub ewa_coeffs_q: [f32; 4], // 16
```

Append exactly these (80 bytes, multiple of 16, every field 4-byte aligned):
```rust
pub cg_flags:     i32,       // 4  - bit0=basic on, bit1=creative on (reserved bits for LUT later)
pub cg_pad0:      i32,       // 8
pub cg_pad1:      i32,       // 12
pub cg_pad2:      i32,       // 16
pub cg_color0:    [f32; 4],  // 16 - (temperature, tint, basic_saturation, exposure)
pub cg_tone0:     [f32; 4],  // 16 - (contrast, highlights, shadows, whites)
pub cg_tone1:     [f32; 4],  // 16 - (blacks, faded_film, vibrance, creative_saturation)
pub cg_reserved:  [f32; 4],  // 16 - reserved for LUT strengths (input_lut, look_lut) in later plan
```

Normalized slider→shader value convention (decided here, used in every task):
- temperature, tint: slider −100..100 → divide by 100 → −1.0..1.0
- basic_saturation, creative_saturation: slider 0..200 → divide by 100 → 0.0..2.0 (1.0 = neutral)
- exposure: slider −100..100 → divide by 100 → −1.0..1.0 (stops = value * 2.0 in shader)
- contrast, highlights, shadows, whites, blacks, vibrance: slider −100..100 → /100 → −1.0..1.0
- faded_film: slider 0..100 → /100 → 0.0..1.0
- cg_flags bit0 set when 基本補正 toggle on; bit1 when クリエイティブ toggle on.

The division by 100 happens in the controller setters (Rust), so the core stores normalized values and the shader receives them directly.

---

## Task 1: ColorGradingParams data struct

**Files:**
- Create: `src/core/color_grading.rs`
- Test: inline `#[cfg(test)]` module in the same file

- [ ] **Step 1: Write the failing test**

Create `src/core/color_grading.rs` with ONLY the test first:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let p = ColorGradingParams::default();
        assert!(!p.basic_enabled);
        assert!(!p.creative_enabled);
        assert_eq!(p.temperature, 0.0);
        assert_eq!(p.basic_saturation, 1.0);
        assert_eq!(p.exposure, 0.0);
        assert_eq!(p.creative_saturation, 1.0);
        assert_eq!(p.faded_film, 0.0);
    }

    #[test]
    fn serde_roundtrip() {
        let mut p = ColorGradingParams::default();
        p.basic_enabled = true;
        p.exposure = 0.5;
        let s = serde_json::to_string(&p).unwrap();
        let p2: ColorGradingParams = serde_json::from_str(&s).unwrap();
        assert_eq!(p2.basic_enabled, true);
        assert_eq!(p2.exposure, 0.5);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src/core && cargo test color_grading::tests`
Expected: FAIL — `cannot find type ColorGradingParams`.

- [ ] **Step 3: Write the struct above the test module**

```rust
/// All values are stored NORMALIZED (shader-ready):
/// temperature/tint/exposure/contrast/highlights/shadows/whites/blacks/vibrance: -1.0..1.0 (0 = neutral)
/// basic_saturation/creative_saturation: 0.0..2.0 (1.0 = neutral)
/// faded_film: 0.0..1.0
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorGradingParams {
    pub basic_enabled: bool,
    pub creative_enabled: bool,

    // Basic - color
    pub temperature: f32,
    pub tint: f32,
    pub basic_saturation: f32,

    // Basic - light
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,

    // Creative
    pub faded_film: f32,
    pub vibrance: f32,
    pub creative_saturation: f32,
}

impl Default for ColorGradingParams {
    fn default() -> Self {
        Self {
            basic_enabled: false,
            creative_enabled: false,
            temperature: 0.0,
            tint: 0.0,
            basic_saturation: 1.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            faded_film: 0.0,
            vibrance: 0.0,
            creative_saturation: 1.0,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src/core && cargo test color_grading::tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Register the module**

In `src/core/lib.rs`, add near the other `pub mod` declarations (e.g. next to `pub mod stabilization;`):

```rust
pub mod color_grading;
```

- [ ] **Step 6: Commit**

```bash
git add src/core/color_grading.rs src/core/lib.rs
git commit -m "feat(color): add ColorGradingParams data struct"
```

---

## Task 2: Wire ColorGradingParams into StabilizationParams + setters

**Files:**
- Modify: `src/core/stabilization_params.rs`
- Modify: `src/core/lib.rs`
- Test: inline test in `src/core/lib.rs` or a new test verifying setters write through

- [ ] **Step 1: Add the field to StabilizationParams**

In `src/core/stabilization_params.rs`, inside `pub struct StabilizationParams { ... }` (after the `focal_length_smoothing_strength: f64,` field), add:

```rust
    pub color_grading: crate::color_grading::ColorGradingParams,
```

In the `impl Default for StabilizationParams` block (after `focal_length_smoothing_strength: 0.5,`), add:

```rust
            color_grading: crate::color_grading::ColorGradingParams::default(),
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src/core && cargo build`
Expected: builds (the `Default` derive on `ColorGradingParams` from Task 1 satisfies the new field).

- [ ] **Step 3: Write the failing test for setters**

Add to `src/core/lib.rs` in a `#[cfg(test)]` module (create one if absent):

```rust
#[cfg(test)]
mod color_grading_setter_tests {
    use crate::StabilizationManager;

    #[test]
    fn setters_write_through() {
        let mgr = StabilizationManager::default();
        mgr.set_cg_exposure(0.5);
        mgr.set_cg_basic_enabled(true);
        let p = mgr.params.read();
        assert_eq!(p.color_grading.exposure, 0.5);
        assert_eq!(p.color_grading.basic_enabled, true);
    }
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cd src/core && cargo test color_grading_setter_tests`
Expected: FAIL — `no method named set_cg_exposure`.

- [ ] **Step 5: Add setter methods on StabilizationManager**

In `src/core/lib.rs`, inside `impl StabilizationManager { ... }` (near `set_background_color`), add one setter per parameter:

```rust
    pub fn set_cg_basic_enabled(&self, v: bool)    { self.params.write().color_grading.basic_enabled = v; }
    pub fn set_cg_creative_enabled(&self, v: bool)  { self.params.write().color_grading.creative_enabled = v; }
    pub fn set_cg_temperature(&self, v: f64)        { self.params.write().color_grading.temperature = v as f32; }
    pub fn set_cg_tint(&self, v: f64)               { self.params.write().color_grading.tint = v as f32; }
    pub fn set_cg_basic_saturation(&self, v: f64)   { self.params.write().color_grading.basic_saturation = v as f32; }
    pub fn set_cg_exposure(&self, v: f64)           { self.params.write().color_grading.exposure = v as f32; }
    pub fn set_cg_contrast(&self, v: f64)           { self.params.write().color_grading.contrast = v as f32; }
    pub fn set_cg_highlights(&self, v: f64)         { self.params.write().color_grading.highlights = v as f32; }
    pub fn set_cg_shadows(&self, v: f64)            { self.params.write().color_grading.shadows = v as f32; }
    pub fn set_cg_whites(&self, v: f64)             { self.params.write().color_grading.whites = v as f32; }
    pub fn set_cg_blacks(&self, v: f64)             { self.params.write().color_grading.blacks = v as f32; }
    pub fn set_cg_faded_film(&self, v: f64)         { self.params.write().color_grading.faded_film = v as f32; }
    pub fn set_cg_vibrance(&self, v: f64)           { self.params.write().color_grading.vibrance = v as f32; }
    pub fn set_cg_creative_saturation(&self, v: f64){ self.params.write().color_grading.creative_saturation = v as f32; }
    pub fn reset_color_grading(&self)               { self.params.write().color_grading = crate::color_grading::ColorGradingParams::default(); }
```

Note: `set_cg_basic_enabled`/`set_cg_creative_enabled` take `bool`; the test calls `set_cg_basic_enabled(true)`. All scalar setters take `f64` and store `as f32` (matching the existing `set_background_color` Vector4<f32> convention).

- [ ] **Step 6: Run test to verify it passes**

Run: `cd src/core && cargo test color_grading_setter_tests`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/core/stabilization_params.rs src/core/lib.rs
git commit -m "feat(color): store color grading in StabilizationParams with setters"
```

---

## Task 3: Append color fields to KernelParams (Rust source of truth)

**Files:**
- Modify: `src/core/stabilization/mod.rs`

- [ ] **Step 1: Append fields to the struct**

In `src/core/stabilization/mod.rs`, in `pub struct KernelParams { ... }`, after `pub ewa_coeffs_q: [f32; 4], // 16`, add:

```rust
    pub cg_flags:     i32,       // 4  - bit0=basic on, bit1=creative on
    pub cg_pad0:      i32,       // 8
    pub cg_pad1:      i32,       // 12
    pub cg_pad2:      i32,       // 16
    pub cg_color0:    [f32; 4],  // 16 - (temperature, tint, basic_saturation, exposure)
    pub cg_tone0:     [f32; 4],  // 16 - (contrast, highlights, shadows, whites)
    pub cg_tone1:     [f32; 4],  // 16 - (blacks, faded_film, vibrance, creative_saturation)
    pub cg_reserved:  [f32; 4],  // 16 - reserved for LUT strengths (later plan)
```

The struct derives `Default` already, and `[f32; 4]`/`i32` are `Default`, so no manual init needed. `bytemuck::Pod`/`Zeroable` remain valid (all fields are POD, packed(4)).

- [ ] **Step 2: Verify it compiles**

Run: `cd src/core && cargo build`
Expected: builds. (If `Default` is not derived but hand-written, add the new fields to the manual `Default` impl with `0` / `[0.0; 4]`.)

- [ ] **Step 3: Commit**

```bash
git add src/core/stabilization/mod.rs
git commit -m "feat(color): append color grading fields to KernelParams"
```

---

## Task 4: Populate KernelParams color fields from params

**Files:**
- Modify: `src/core/stabilization/compute_params.rs`
- Modify: `src/core/stabilization/frame_transform.rs`

- [ ] **Step 1: Carry color_grading into ComputeParams**

In `src/core/stabilization/compute_params.rs`, find `pub struct ComputeParams { ... }` and add:

```rust
    pub color_grading: crate::color_grading::ColorGradingParams,
```

In `ComputeParams::from_manager` (where other `params.xxx` fields are copied from the read lock), add:

```rust
        color_grading: params.color_grading,
```

(If `ComputeParams` derives `Default` and `from_manager` uses struct-update syntax `..Default::default()`, the `ColorGradingParams: Default` from Task 1 covers it; still set it explicitly from `params` so the value is real, not default.)

- [ ] **Step 2: Populate KernelParams in frame_transform.rs**

In `src/core/stabilization/frame_transform.rs`, locate where `kernel_params` is constructed/filled for a `FrameTransform` (the function that sets `kernel_params.background`, `kernel_params.fov`, etc. from `params`). After those assignments, add:

```rust
        {
            let cg = &params.color_grading;
            let mut flags = 0i32;
            if cg.basic_enabled    { flags |= 1; }
            if cg.creative_enabled { flags |= 2; }
            kernel_params.cg_flags  = flags;
            kernel_params.cg_color0 = [cg.temperature, cg.tint, cg.basic_saturation, cg.exposure];
            kernel_params.cg_tone0  = [cg.contrast, cg.highlights, cg.shadows, cg.whites];
            kernel_params.cg_tone1  = [cg.blacks, cg.faded_film, cg.vibrance, cg.creative_saturation];
        }
```

Note: use the exact local variable name used for the params reference in that function (it may be `params`, `comp_params`, or via `self`). Match the surrounding code that reads `params.background`.

- [ ] **Step 3: Verify it compiles**

Run: `cd src/core && cargo build`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add src/core/stabilization/compute_params.rs src/core/stabilization/frame_transform.rs
git commit -m "feat(color): plumb color grading into KernelParams per frame"
```

---

## Task 5: Mirror new fields in WGSL and OpenCL param structs (layout sync)

**Files:**
- Modify: `src/core/gpu/wgpu_undistort.wgsl`
- Modify: `src/core/gpu/opencl_undistort.cl`

These are layout-sync only — the export shaders must not break because the uploaded `KernelParams` buffer is now larger. No color math here (that's the export plan).

- [ ] **Step 1: Mirror in WGSL**

In `src/core/gpu/wgpu_undistort.wgsl`, in `struct KernelParams { ... }`, after `ewa_coeffs_q: vec4<f32>, // 16`, add:

```wgsl
    cg_flags:    i32, // 4
    cg_pad0:     i32, // 8
    cg_pad1:     i32, // 12
    cg_pad2:     i32, // 16
    cg_color0:   vec4<f32>, // 16
    cg_tone0:    vec4<f32>, // 16
    cg_tone1:    vec4<f32>, // 16
    cg_reserved: vec4<f32>, // 16
```

- [ ] **Step 2: Mirror in OpenCL**

In `src/core/gpu/opencl_undistort.cl`, find the `struct KernelParams` (mirrors the Rust struct, ends with `ewa_coeffs_p` / `ewa_coeffs_q`). After the last field, add the equivalent:

```c
    int cg_flags;       // 4
    int cg_pad0;        // 8
    int cg_pad1;        // 12
    int cg_pad2;        // 16
    float4 cg_color0;   // 16
    float4 cg_tone0;    // 16
    float4 cg_tone1;    // 16
    float4 cg_reserved; // 16
```

(Use the same float-vector type the file already uses for `ewa_coeffs_p`, e.g. `float4`.)

- [ ] **Step 3: Verify it builds (workspace)**

Run: `cargo build` (from repo root)
Expected: builds. WGSL is validated at runtime; a quick run is in Task 10. OpenCL compiles at runtime only — visual check the field names/types match the existing pattern in the file.

- [ ] **Step 4: Commit**

```bash
git add src/core/gpu/wgpu_undistort.wgsl src/core/gpu/opencl_undistort.cl
git commit -m "chore(color): mirror color grading fields in wgsl/opencl param structs"
```

---

## Task 6: GLSL color grading in the preview shader

**Files:**
- Modify: `src/qt_gpu/undistort.frag`

- [ ] **Step 1: Mirror the new fields in the `buf` uniform block**

In `src/qt_gpu/undistort.frag`, inside `layout(std140, binding = 1) uniform buf { ... }`, after `vec4 params_ewa_coeffs_q;`, add:

```glsl
    int params_cg_flags;
    int params_cg_pad0;
    int params_cg_pad1;
    int params_cg_pad2;
    vec4 params_cg_color0;
    vec4 params_cg_tone0;
    vec4 params_cg_tone1;
    vec4 params_cg_reserved;
```

- [ ] **Step 2: Add the color grading function**

Add this function in `undistort.frag` above the `main()` function (or above the function that returns the final color):

```glsl
vec3 apply_color_grading(vec3 c) {
    // Inputs are normalized (see plan). Work in the shader's pixel value scale,
    // so normalize to 0..1 using params_max_pixel_value, grade, then scale back.
    float mpv = max(params_max_pixel_value, 1.0);
    vec3 x = clamp(c / mpv, 0.0, 1.0);

    // ---- 基本補正 (basic) ----
    if ((params_cg_flags & 1) != 0) {
        float temperature = params_cg_color0.x; // -1..1
        float tint        = params_cg_color0.y; // -1..1
        float saturation  = params_cg_color0.z; // 0..2
        float exposure    = params_cg_color0.w; // -1..1
        float contrast    = params_cg_tone0.x;  // -1..1
        float highlights  = params_cg_tone0.y;  // -1..1
        float shadows     = params_cg_tone0.z;  // -1..1
        float whites      = params_cg_tone0.w;  // -1..1
        float blacks      = params_cg_tone1.x;  // -1..1

        // White balance: warm/cool on R/B, tint on G/magenta
        x.r += temperature * 0.2;
        x.b -= temperature * 0.2;
        x.g += tint * 0.2;

        // Exposure (stops): value*2 stops
        x *= pow(2.0, exposure * 2.0);

        // Contrast around 0.5 midpoint
        x = (x - 0.5) * (1.0 + contrast) + 0.5;

        // Tone: highlights lift brights, shadows lift darks, whites/blacks endpoints
        float luma = dot(x, vec3(0.2126, 0.7152, 0.0722));
        x += highlights * 0.5 * smoothstep(0.5, 1.0, luma);
        x += shadows    * 0.5 * (1.0 - smoothstep(0.0, 0.5, luma));
        x += whites * 0.2 * luma;
        x += blacks * 0.2 * (1.0 - luma);

        // Saturation
        float g = dot(x, vec3(0.2126, 0.7152, 0.0722));
        x = mix(vec3(g), x, saturation);
    }

    // ---- クリエイティブ (creative) ----
    if ((params_cg_flags & 2) != 0) {
        float faded_film          = params_cg_tone1.y; // 0..1
        float vibrance            = params_cg_tone1.z; // -1..1
        float creative_saturation = params_cg_tone1.w; // 0..2

        // Faded film: lift blacks toward a soft floor
        x = mix(x, x * (1.0 - 0.15) + 0.15, faded_film);

        // Vibrance: scale saturation more for less-saturated pixels
        float g2 = dot(x, vec3(0.2126, 0.7152, 0.0722));
        float sat = length(x - vec3(g2));
        float vib = vibrance * (1.0 - smoothstep(0.0, 0.6, sat));
        x = mix(vec3(g2), x, 1.0 + vib);

        // Creative saturation
        float g3 = dot(x, vec3(0.2126, 0.7152, 0.0722));
        x = mix(vec3(g3), x, creative_saturation);
    }

    return clamp(x, 0.0, 1.0) * mpv;
}
```

- [ ] **Step 3: Apply it to the final color**

In `undistort.frag`, find where the final output color is written (`fragColor = ...;`). Wrap the RGB:

```glsl
    fragColor.rgb = apply_color_grading(fragColor.rgb);
```

Place this as the LAST modification to `fragColor` before the shader ends. If the final value comes from a helper returning `vec4`, apply to its `.rgb` after assignment.

- [ ] **Step 4: Confirm the shader is rebuilt**

The `.frag` is compiled to `.qsb` by the build (`build.rs`). Run:

Run: `cargo build 2>&1 | tail -20`
Expected: build succeeds and the qt_gpu shader is recompiled (no GLSL compile errors printed). If the build caches shaders, `touch src/qt_gpu/undistort.frag` then rebuild.

- [ ] **Step 5: Commit**

```bash
git add src/qt_gpu/undistort.frag
git commit -m "feat(color): apply color grading in preview fragment shader"
```

---

## Task 7: Controller bridge (Qt methods)

**Files:**
- Modify: `src/controller.rs`

- [ ] **Step 1: Add qt_method declarations**

In `src/controller.rs`, in the struct where `set_background_color: qt_method!(...)` is declared, add:

```rust
    set_cg_basic_enabled:       qt_method!(fn(&self, v: bool)),
    set_cg_creative_enabled:    qt_method!(fn(&self, v: bool)),
    set_cg_temperature:         qt_method!(fn(&self, v: f64)),
    set_cg_tint:                qt_method!(fn(&self, v: f64)),
    set_cg_basic_saturation:    qt_method!(fn(&self, v: f64)),
    set_cg_exposure:            qt_method!(fn(&self, v: f64)),
    set_cg_contrast:            qt_method!(fn(&self, v: f64)),
    set_cg_highlights:          qt_method!(fn(&self, v: f64)),
    set_cg_shadows:             qt_method!(fn(&self, v: f64)),
    set_cg_whites:              qt_method!(fn(&self, v: f64)),
    set_cg_blacks:              qt_method!(fn(&self, v: f64)),
    set_cg_faded_film:          qt_method!(fn(&self, v: f64)),
    set_cg_vibrance:            qt_method!(fn(&self, v: f64)),
    set_cg_creative_saturation: qt_method!(fn(&self, v: f64)),
    reset_color_grading:        qt_method!(fn(&self)),
```

- [ ] **Step 2: Add the implementations via wrap_simple_method!**

In `src/controller.rs`, near the other `wrap_simple_method!` blocks (e.g. around the `set_fov` group), add:

```rust
    wrap_simple_method!(set_cg_basic_enabled,       v: bool; recompute);
    wrap_simple_method!(set_cg_creative_enabled,    v: bool; recompute);
    wrap_simple_method!(set_cg_temperature,         v: f64; recompute);
    wrap_simple_method!(set_cg_tint,                v: f64; recompute);
    wrap_simple_method!(set_cg_basic_saturation,    v: f64; recompute);
    wrap_simple_method!(set_cg_exposure,            v: f64; recompute);
    wrap_simple_method!(set_cg_contrast,            v: f64; recompute);
    wrap_simple_method!(set_cg_highlights,          v: f64; recompute);
    wrap_simple_method!(set_cg_shadows,             v: f64; recompute);
    wrap_simple_method!(set_cg_whites,              v: f64; recompute);
    wrap_simple_method!(set_cg_blacks,              v: f64; recompute);
    wrap_simple_method!(set_cg_faded_film,          v: f64; recompute);
    wrap_simple_method!(set_cg_vibrance,            v: f64; recompute);
    wrap_simple_method!(set_cg_creative_saturation, v: f64; recompute);
    wrap_simple_method!(reset_color_grading,        ; recompute);
```

Note: `wrap_simple_method!` calls `self.stabilizer.<name>(...)` then `self.request_recompute()`. The names match the `StabilizationManager` methods from Task 2. `reset_color_grading` takes no params — confirm the macro's no-param arm accepts `(reset_color_grading, ; recompute)`; if the macro requires at least one param pattern, write the method by hand:

```rust
    fn reset_color_grading(&self) {
        self.stabilizer.reset_color_grading();
        self.request_recompute();
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | tail -20`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add src/controller.rs
git commit -m "feat(color): expose color grading setters to QML"
```

---

## Task 8: QML color grading panel

**Files:**
- Create: `src/ui/menu/ColorGrading.qml`
- Modify: `src/ui/menu/qmldir`
- Modify: `src/ui/App.qml`

- [ ] **Step 1: Create the menu component**

Create `src/ui/menu/ColorGrading.qml`:

```qml
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Controls as QQC
import "../components/"

MenuItem {
    id: root;
    text: qsTr("基本補正");
    iconName: "color";
    objectName: "colorGrading";
    innerItem.enabled: window.videoArea.vid.loaded;

    function sliderRow(parent) { } // placeholder removed below

    Item {
        id: sett;
        property alias basicEnabled: basicEnabled.checked;
        property alias creativeEnabled: creativeEnabled.checked;
        property alias temperature: temperature.value;
        property alias tint: tint.value;
        property alias basicSaturation: basicSaturation.value;
        property alias exposure: exposure.value;
        property alias contrast: contrast.value;
        property alias highlights: highlights.value;
        property alias shadows: shadows.value;
        property alias whites: whites.value;
        property alias blacks: blacks.value;
        property alias fadedFilm: fadedFilm.value;
        property alias vibrance: vibrance.value;
        property alias creativeSaturation: creativeSaturation.value;
        Component.onCompleted: settings.init(sett);
        function propChanged() { settings.propChanged(sett); }
    }

    Column {
        width: parent.width;
        spacing: 8 * dpiScale;

        CheckBox {
            id: basicEnabled;
            text: qsTr("基本補正を有効化");
            checked: false;
            onCheckedChanged: controller.set_cg_basic_enabled(checked);
        }

        BasicText { text: qsTr("カラー"); }

        Label {
            text: qsTr("色温度"); width: parent.width;
            SliderWithField { id: temperature; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_temperature(value / 100.0); }
        }
        Label {
            text: qsTr("色かぶり補正"); width: parent.width;
            SliderWithField { id: tint; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_tint(value / 100.0); }
        }
        Label {
            text: qsTr("彩度"); width: parent.width;
            SliderWithField { id: basicSaturation; from: 0; to: 200; value: 100; defaultValue: 100; precision: 0; width: parent.width; onValueChanged: controller.set_cg_basic_saturation(value / 100.0); }
        }

        BasicText { text: qsTr("ライト"); }

        Label {
            text: qsTr("露光量"); width: parent.width;
            SliderWithField { id: exposure; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_exposure(value / 100.0); }
        }
        Label {
            text: qsTr("コントラスト"); width: parent.width;
            SliderWithField { id: contrast; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_contrast(value / 100.0); }
        }
        Label {
            text: qsTr("ハイライト"); width: parent.width;
            SliderWithField { id: highlights; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_highlights(value / 100.0); }
        }
        Label {
            text: qsTr("シャドウ"); width: parent.width;
            SliderWithField { id: shadows; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_shadows(value / 100.0); }
        }
        Label {
            text: qsTr("白レベル"); width: parent.width;
            SliderWithField { id: whites; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_whites(value / 100.0); }
        }
        Label {
            text: qsTr("黒レベル"); width: parent.width;
            SliderWithField { id: blacks; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_blacks(value / 100.0); }
        }

        Button {
            text: qsTr("リセット");
            onClicked: {
                controller.reset_color_grading();
                temperature.value = 0; tint.value = 0; basicSaturation.value = 100;
                exposure.value = 0; contrast.value = 0; highlights.value = 0;
                shadows.value = 0; whites.value = 0; blacks.value = 0;
                fadedFilm.value = 0; vibrance.value = 0; creativeSaturation.value = 100;
            }
        }

        Hr { }

        CheckBox {
            id: creativeEnabled;
            text: qsTr("クリエイティブを有効化");
            checked: false;
            onCheckedChanged: controller.set_cg_creative_enabled(checked);
        }
        Label {
            text: qsTr("色あせたフィルム"); width: parent.width;
            SliderWithField { id: fadedFilm; from: 0; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_faded_film(value / 100.0); }
        }
        Label {
            text: qsTr("自然な彩度"); width: parent.width;
            SliderWithField { id: vibrance; from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width; onValueChanged: controller.set_cg_vibrance(value / 100.0); }
        }
        Label {
            text: qsTr("彩度"); width: parent.width;
            SliderWithField { id: creativeSaturation; from: 0; to: 200; value: 100; defaultValue: 100; precision: 0; width: parent.width; onValueChanged: controller.set_cg_creative_saturation(value / 100.0); }
        }
    }
}
```

Remove the stray `function sliderRow(parent) { }` line — it was an editing artifact; the component does not need it. (Delete that line before saving.)

- [ ] **Step 2: Register in qmldir**

In `src/ui/menu/qmldir`, add a line (alphabetical with the others):

```
ColorGrading 1.0 ColorGrading.qml
```

- [ ] **Step 3: Register in App.qml**

In `src/ui/App.qml`, inside the right `SidePanel { ... }`, after the Stabilization loader block:

```qml
    ItemLoader { id: stab; sourceComponent: Component { Menu.Stabilization { } } }
    Hr { id: stabHr; }
    ItemLoader { id: colorGrading; sourceComponent: Component { Menu.ColorGrading { } } }
    Hr { id: colorGradingHr; }
```

(Insert the two new lines after the existing `Hr { id: stabHr; }`.)

- [ ] **Step 4: Build and launch to verify the panel appears**

Run: `cargo build 2>&1 | tail -20`
Expected: builds.

Then launch the app (see Task 10) and confirm the "基本補正" section appears in the right panel with all sliders. If `iconName: "color"` shows no icon, that's cosmetic — leave it (a missing SVG just renders blank); a real icon can be added later.

- [ ] **Step 5: Commit**

```bash
git add src/ui/menu/ColorGrading.qml src/ui/menu/qmldir src/ui/App.qml
git commit -m "feat(color): add color grading panel to right side panel"
```

---

## Task 9: Persistence in .gyroflow project file

**Files:**
- Modify: `src/core/lib.rs` (`export_gyroflow_data` and the import/load function)

- [ ] **Step 1: Serialize on export**

In `src/core/lib.rs`, inside `export_gyroflow_data`, in the `serde_json::json!({ ... })` object, add a `"color_grading"` key inside the `"stabilization"` sub-object (next to `"lens_correction_amount": params.lens_correction_amount,`):

```rust
            "color_grading": serde_json::to_value(&params.color_grading).unwrap_or(serde_json::Value::Null),
```

- [ ] **Step 2: Deserialize on import**

In `src/core/lib.rs`, find the import/load function (the counterpart that reads the `"stabilization"` object back, e.g. `import_gyroflow_data` / `load_gyroflow_data`). Where other stabilization fields are restored, add:

```rust
        if let Some(cg) = obj["stabilization"]["color_grading"].as_object() {
            if let Ok(parsed) = serde_json::from_value::<crate::color_grading::ColorGradingParams>(serde_json::Value::Object(cg.clone())) {
                self.params.write().color_grading = parsed;
            }
        }
```

Match the exact accessor pattern used nearby (the file may bind the parsed root to a local `obj` or `v`; use the same). If stabilization fields are read via a typed struct rather than `obj[...]`, add `color_grading: Option<ColorGradingParams>` to that struct instead and assign it.

- [ ] **Step 3: Write a roundtrip test**

Add to the `#[cfg(test)]` area in `src/core/lib.rs`:

```rust
#[cfg(test)]
mod color_grading_persist_tests {
    use crate::StabilizationManager;
    #[test]
    fn export_contains_color_grading() {
        let mgr = StabilizationManager::default();
        mgr.set_cg_exposure(0.42);
        mgr.set_cg_basic_enabled(true);
        let json = mgr.export_gyroflow_data(crate::GyroflowProjectType::WithGyroData, "", None).unwrap();
        assert!(json.contains("color_grading"));
        assert!(json.contains("0.42"));
    }
}
```

(Adjust `GyroflowProjectType::WithGyroData` to a real variant — check the enum; use whatever variant the existing tests/callers use.)

- [ ] **Step 4: Run the test**

Run: `cd src/core && cargo test color_grading_persist_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/core/lib.rs
git commit -m "feat(color): persist color grading in .gyroflow project"
```

---

## Task 10: Build, run, and manually verify the preview

**Files:** none (verification)

- [ ] **Step 1: Full build**

Run: `cargo build 2>&1 | tail -30`
Expected: builds with no errors.

- [ ] **Step 2: Run core tests**

Run: `cd src/core && cargo test color_grading`
Expected: all color grading tests pass.

- [ ] **Step 3: Launch the app and verify live preview**

Launch Gyroflow (use the project's run method — e.g. `cargo run --release`, or the `run` skill). Then:
1. Open any video so the preview shows a frame.
2. Open the "基本補正" section in the right panel.
3. Enable "基本補正を有効化".
4. Drag 露光量 (exposure) — the preview should brighten/darken in real time.
5. Drag 彩度 (saturation) to 0 — preview should go grayscale.
6. Drag 色温度 (temperature) — preview should warm/cool.
7. Enable "クリエイティブを有効化" and drag 自然な彩度 — preview should change.
8. Click リセット — preview returns to original and sliders reset.

Expected: each adjustment is visible in the preview. If nothing changes, verify `request_recompute()` fires (it should, via `wrap_simple_method!; recompute`) and that the preview path is qt_gpu (the `.frag` shader) — confirm the build recompiled `undistort.frag` to `.qsb`.

- [ ] **Step 4: Verify project save/load**

Save a `.gyroflow` project with grading applied, reload it, confirm the sliders and preview restore.

- [ ] **Step 5: Final commit (if any fixups)**

```bash
git add -A
git commit -m "test(color): verify preview color grading end to end"
```

---

## Self-Review notes (addressed)

- **Spec coverage:** This plan covers the スカラー adjustments of 基本補正 + クリエイティブ for the **preview** path. LUT (`.cube`, 2 slots) and **export baking** are explicitly deferred to follow-on plans below (they require GPU texture bindings / YUV handling that depend on Qt RHI C++ not fully readable at planning time). The spec's "書き出しも必ず対応" requirement is preserved as a required follow-on plan, not dropped.
- **Type consistency:** setter names (`set_cg_*`) are identical across `StabilizationManager` (Task 2), `wrap_simple_method!` (Task 7), and qt declarations (Task 7). `KernelParams` field names (`cg_flags`, `cg_color0`, `cg_tone0`, `cg_tone1`, `cg_reserved`) are identical across Rust (Task 3), WGSL/OpenCL (Task 5), GLSL (Task 6), and population (Task 4). Normalization (÷100 in QML, store f32 in core, consume directly in shader) is consistent.
- **No placeholders:** every code step has concrete code. The one editing artifact (`function sliderRow`) is explicitly flagged for deletion.

---

## Follow-on plans (NOT in this plan — to be written next)

1. **`.cube` LUT support (preview).** Add `src/core/lut.rs` (`.cube` 1D/3D parser → normalized 3D table), 2 LUT slots in `ColorGradingParams` (path + strength + enabled), a 3D texture/sampler binding in `src/qt_gpu/qrhi_undistort.cpp` / `.h` / `.rs` (Qt RHI — requires reading that C++, currently scanner-blocked), `set_lut_file(slot, path)` in controller, and FileDialog UI in `ColorGrading.qml` (LUT設定 / ルック). Apply LUT lookups in `undistort.frag` using `cg_reserved` strengths.

2. **Export baking (wgpu + CPU).** Apply the SAME color math in `src/core/gpu/wgpu_undistort.wgsl` and `src/core/stabilization/cpu_undistort.rs` as a full-RGBA pass. For YUV output formats (NV12/P010/YUV*), insert YUV→RGB→grade→YUV around the color pass (per-plane processing means a dedicated full-frame RGBA stage is required). **This fulfills the user's explicit "書き出しも必ず対応" requirement.**

3. **(Optional) OpenCL color math + LUT** for parity on OpenCL export.
