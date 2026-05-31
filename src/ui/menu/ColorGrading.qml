// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2026 Gyroflow

import QtQuick
import QtQuick.Controls as QQC

import "../components/"

MenuItem {
    id: root;
    text: qsTr("基本補正");
    iconName: "color";
    objectName: "colorGrading";
    innerItem.enabled: window.videoArea.vid.loaded;

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

    CheckBox {
        id: basicEnabled;
        text: qsTr("基本補正を有効化");
        checked: false;
        onCheckedChanged: { controller.set_cg_basic_enabled(checked); sett.propChanged(); }
    }

    BasicText { text: qsTr("カラー"); }

    Label {
        text: qsTr("色温度");
        width: parent.width;
        SliderWithField {
            id: temperature;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_temperature(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("色かぶり補正");
        width: parent.width;
        SliderWithField {
            id: tint;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_tint(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("彩度");
        width: parent.width;
        SliderWithField {
            id: basicSaturation;
            from: 0; to: 200; value: 100; defaultValue: 100; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_basic_saturation(value / 100.0); sett.propChanged(); }
        }
    }

    BasicText { text: qsTr("ライト"); }

    Label {
        text: qsTr("露光量");
        width: parent.width;
        SliderWithField {
            id: exposure;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_exposure(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("コントラスト");
        width: parent.width;
        SliderWithField {
            id: contrast;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_contrast(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("ハイライト");
        width: parent.width;
        SliderWithField {
            id: highlights;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_highlights(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("シャドウ");
        width: parent.width;
        SliderWithField {
            id: shadows;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_shadows(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("白レベル");
        width: parent.width;
        SliderWithField {
            id: whites;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_whites(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("黒レベル");
        width: parent.width;
        SliderWithField {
            id: blacks;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_blacks(value / 100.0); sett.propChanged(); }
        }
    }

    Button {
        text: qsTr("リセット");
        anchors.right: parent.right;
        onClicked: {
            controller.reset_color_grading();
            temperature.value = 0; tint.value = 0; basicSaturation.value = 100;
            exposure.value = 0; contrast.value = 0; highlights.value = 0;
            shadows.value = 0; whites.value = 0; blacks.value = 0;
            fadedFilm.value = 0; vibrance.value = 0; creativeSaturation.value = 100;
            sett.propChanged();
        }
    }

    Hr { }

    CheckBox {
        id: creativeEnabled;
        text: qsTr("クリエイティブを有効化");
        checked: false;
        onCheckedChanged: { controller.set_cg_creative_enabled(checked); sett.propChanged(); }
    }

    BasicText { text: qsTr("調整"); }

    Label {
        text: qsTr("色あせたフィルム");
        width: parent.width;
        SliderWithField {
            id: fadedFilm;
            from: 0; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_faded_film(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("自然な彩度");
        width: parent.width;
        SliderWithField {
            id: vibrance;
            from: -100; to: 100; value: 0; defaultValue: 0; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_vibrance(value / 100.0); sett.propChanged(); }
        }
    }
    Label {
        text: qsTr("彩度");
        width: parent.width;
        SliderWithField {
            id: creativeSaturation;
            from: 0; to: 200; value: 100; defaultValue: 100; precision: 0; width: parent.width;
            onValueChanged: { controller.set_cg_creative_saturation(value / 100.0); sett.propChanged(); }
        }
    }
}
