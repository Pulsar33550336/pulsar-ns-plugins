import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Widgets
import qs.Services.UI
import "." as Local

ColumnLayout {
  id: root
  spacing: 0

  property var pluginApi: null
  property var cfg: pluginApi?.pluginSettings || ({})
  property var defaults: pluginApi?.manifest?.metadata?.defaultSettings || ({})

  // Live preview + revert-on-cancel pattern (approved deviation from edit-copy).
  // Color settings apply visually in real time via _applyPreview, but the snapshot
  // is restored on close if saveSettings() (Apply) was never called.
  property var _snapshot: null
  property bool _applied: false

  function _applyPreview(key, value) {
    if (!pluginApi) return;
    var patch = {};
    patch[key] = value;
    pluginApi.pluginSettings = Object.assign({}, pluginApi.pluginSettings, patch);
  }

  // Edit-copy properties for non-preview settings (window, layout, parsing, paths)
  property int editWindowWidth: cfg.windowWidth ?? defaults.windowWidth ?? 1400
  property int editWindowHeight: cfg.windowHeight ?? defaults.windowHeight ?? 850
  property bool editAutoHeight: cfg.autoHeight ?? defaults.autoHeight ?? true
  property int editColumnCount: cfg.columnCount ?? defaults.columnCount ?? 3
  property string editDummy: ""
  property bool editShowUndescribedBinds: cfg.showUndescribedBinds ?? defaults.showUndescribedBinds ?? false

  // Symbol display settings
  property bool editUseMacSymbol: cfg.useMacSymbol ?? defaults.useMacSymbol ?? false
  property bool editUseFnSymbol: cfg.useFnSymbol ?? defaults.useFnSymbol ?? false
  property bool editUseMouseSymbol: cfg.useMouseSymbol ?? defaults.useMouseSymbol ?? false
  property string editSuperKeyText: cfg.superKeyText ?? defaults.superKeyText ?? ""
  property bool editSplitButtons: cfg.splitButtons ?? defaults.splitButtons ?? true

  // Live-preview properties for colors (mirror current pluginSettings; written via _applyPreview).
  // All background color fields are stored as string (hex) for empty-string support.
  property string valueKeyColorSuper:   cfg.keyColorSuper   ?? defaults.keyColorSuper   ?? ""
  property string valueKeyColorCtrl:    cfg.keyColorCtrl    ?? defaults.keyColorCtrl    ?? ""
  property string valueKeyColorShift:   cfg.keyColorShift   ?? defaults.keyColorShift   ?? ""
  property string valueKeyColorAlt:     cfg.keyColorAlt     ?? defaults.keyColorAlt     ?? "#FF6B6B"
  property string valueKeyColorXF86:    cfg.keyColorXF86    ?? defaults.keyColorXF86    ?? "#4ECDC4"
  property string valueKeyColorPrint:   cfg.keyColorPrint   ?? defaults.keyColorPrint   ?? "#95E1D3"
  property string valueKeyColorNumeric: cfg.keyColorNumeric ?? defaults.keyColorNumeric ?? "#A8DADC"
  property string valueKeyColorMouse:   cfg.keyColorMouse   ?? defaults.keyColorMouse   ?? "#F38181"
  property string valueKeyColorDefault: cfg.keyColorDefault ?? defaults.keyColorDefault ?? "#6C757D"
  // Per-category text color overrides — empty string = fall back to keyLabelColor global
  property string valueKeyTextSuper:   cfg.keyTextSuper   ?? defaults.keyTextSuper   ?? ""
  property string valueKeyTextCtrl:    cfg.keyTextCtrl    ?? defaults.keyTextCtrl    ?? ""
  property string valueKeyTextShift:   cfg.keyTextShift   ?? defaults.keyTextShift   ?? ""
  property string valueKeyTextAlt:     cfg.keyTextAlt     ?? defaults.keyTextAlt     ?? ""
  property string valueKeyTextXF86:    cfg.keyTextXF86    ?? defaults.keyTextXF86    ?? ""
  property string valueKeyTextPrint:   cfg.keyTextPrint   ?? defaults.keyTextPrint   ?? ""
  property string valueKeyTextNumeric: cfg.keyTextNumeric ?? defaults.keyTextNumeric ?? ""
  property string valueKeyTextMouse:   cfg.keyTextMouse   ?? defaults.keyTextMouse   ?? ""
  property string valueKeyTextDefault: cfg.keyTextDefault ?? defaults.keyTextDefault ?? ""
  property string valueKeyLabelColor:  cfg.keyLabelColor  ?? defaults.keyLabelColor  ?? "#FFFFFF"
  property string valueDescriptionTextColor: cfg.descriptionTextColor ?? defaults.descriptionTextColor ?? "#E0E0E0"

  // Clipboard polling for quick-paste feature.
  // Reads wl-paste periodically while Settings is visible; exposes a validated hex if found.
  property string clipboardHex: ""

  Timer {
    id: clipboardTimer
    interval: 1500
    running: root.visible
    repeat: true
    triggeredOnStart: true
    onTriggered: if (!clipboardProcess.running) clipboardProcess.running = true
  }

  Process {
    id: clipboardProcess
    command: ["wl-paste", "-n"]
    stdout: StdioCollector {
      onStreamFinished: {
        var raw = (text || "").trim();
        // Accept #RRGGBB or #RRGGBBAA (with or without leading #)
        var m = raw.match(/^#?([0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?)$/);
        if (m) {
          root.clipboardHex = "#" + m[1].toUpperCase();
        } else {
          root.clipboardHex = "";
        }
      }
    }
  }

  Component.onCompleted: {
    if (pluginApi) {
      _snapshot = JSON.parse(JSON.stringify(pluginApi.pluginSettings));
    }
  }

  Component.onDestruction: {
    clipboardTimer.stop();
    if (clipboardProcess.running) clipboardProcess.running = false;
    if (!_applied && pluginApi && _snapshot) {
      pluginApi.pluginSettings = Object.assign({}, _snapshot);
    }
  }

  // ============ TAB BAR ============
  NTabBar {
    id: tabBar
    Layout.fillWidth: true
    Layout.bottomMargin: Style.marginM
    distributeEvenly: true
    currentIndex: tabView.currentIndex

    NTabButton {
      text: pluginApi?.tr("settings.tab-general")
      icon: "settings"
      tabIndex: 0
      checked: tabBar.currentIndex === 0
    }
    NTabButton {
      text: pluginApi?.tr("settings.tab-appearance")
      icon: "color-picker"
      tabIndex: 1
      checked: tabBar.currentIndex === 1
    }
  }

  Item {
    Layout.fillWidth: true
    Layout.preferredHeight: Style.marginS
  }

  // ============ TAB VIEW ============
  NTabView {
    id: tabView
    currentIndex: tabBar.currentIndex

    // ============ TAB 1: GENERAL ============
    ColumnLayout {
      id: generalTab
      spacing: Style.marginM

      // Header
      NText {
        Layout.fillWidth: true
        text: pluginApi?.tr("settings.title")
        pointSize: Style.fontSizeXXL
        font.weight: Style.fontWeightBold
        color: Color.mOnSurface
      }

      NText {
        Layout.fillWidth: true
        text: pluginApi?.tr("settings.description")
        color: Color.mOnSurfaceVariant
        pointSize: Style.fontSizeM
        wrapMode: Text.WordWrap
      }

      // ===== Window Size =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: sizeContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: sizeContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginS

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.window-size")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          RowLayout {
            spacing: Style.marginM

            NText {
              text: pluginApi?.tr("settings.width")
              color: Color.mOnSurface
              pointSize: Style.fontSizeM
              Layout.preferredWidth: 120
            }

            NTextInput {
              id: widthInput
              Layout.preferredWidth: 100 * Style.uiScaleRatio
              Layout.preferredHeight: Style.baseWidgetSize
              text: root.editWindowWidth.toString()
              onTextChanged: {
                var val = parseInt(text);
                if (!isNaN(val) && val >= 400 && val <= 3000) {
                  root.editWindowWidth = val;
                }
              }
            }

            NText {
              text: pluginApi?.tr("settings.width-range")
              color: Color.mOnSurfaceVariant
              pointSize: Style.fontSizeS
            }
          }

          RowLayout {
            spacing: Style.marginM

            NText {
              text: pluginApi?.tr("settings.height")
              color: Color.mOnSurface
              pointSize: Style.fontSizeM
              Layout.preferredWidth: 120
            }

            NToggle {
              id: autoHeightToggle
              label: pluginApi?.tr("settings.auto-height")
              checked: root.editAutoHeight
              onToggled: function(checked) {
                root.editAutoHeight = checked;
              }
            }

            NTextInput {
              id: heightInput
              Layout.preferredWidth: 100 * Style.uiScaleRatio
              Layout.preferredHeight: Style.baseWidgetSize
              Component.onCompleted: text = root.editWindowHeight > 0 ? root.editWindowHeight.toString() : "850"
              enabled: !autoHeightToggle.checked
              opacity: enabled ? 1.0 : 0.5
              onTextChanged: {
                var val = parseInt(text);
                if (!isNaN(val) && val >= 300 && val <= 2000) {
                  root.editWindowHeight = val;
                }
              }
            }

            NText {
              text: pluginApi?.tr("settings.height-range")
              color: Color.mOnSurfaceVariant
              pointSize: Style.fontSizeS
              opacity: autoHeightToggle.checked ? 0.5 : 1.0
            }
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.auto-height-hint")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeS
            wrapMode: Text.WordWrap
          }
        }
      }

      // ===== Layout (Columns) =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: columnContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: columnContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginS

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.layout")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          RowLayout {
            spacing: Style.marginM

            NText {
              text: pluginApi?.tr("settings.columns")
              color: Color.mOnSurface
              pointSize: Style.fontSizeM
            }

            NComboBox {
              id: columnCombo
              Layout.preferredWidth: 150 * Style.uiScaleRatio
              Layout.preferredHeight: Style.baseWidgetSize
              model: ListModel {
                ListElement { name: "1"; key: "1" }
                ListElement { name: "2"; key: "2" }
                ListElement { name: "3"; key: "3" }
                ListElement { name: "4"; key: "4" }
              }
              currentKey: root.editColumnCount.toString()
              onSelected: key => {
                root.editColumnCount = parseInt(key);
              }
            }
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.columns-hint")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeS
            wrapMode: Text.WordWrap
          }
        }
      }

      // ===== Parsing Options =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: parsingContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: parsingContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginS

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.parsing-options")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          NToggle {
            Layout.fillWidth: true
            label: pluginApi?.tr("settings.show-undescribed")
            description: pluginApi?.tr("settings.show-undescribed-hint")
            checked: root.editShowUndescribedBinds
            onToggled: function(checked) {
              root.editShowUndescribedBinds = checked;
            }
          }

        }
      }

      // ===== Actions =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: actionsContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: actionsContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginM

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.actions")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          RowLayout {
            spacing: Style.marginM

            NButton {
              text: pluginApi?.tr("settings.refresh-keybinds")
              icon: "refresh"
              onClicked: {
                pluginApi?.mainInstance?.refresh();
                ToastService.showNotice(pluginApi?.tr("settings.refresh-message"));
              }
            }

            NButton {
              text: pluginApi?.tr("settings.reset-defaults")
              icon: "rotate"
              onClicked: {
                root.editWindowWidth = defaults.windowWidth || 1400;
                root.editWindowHeight = defaults.windowHeight || 850;
                root.editAutoHeight = defaults.autoHeight ?? true;
                root.editColumnCount = defaults.columnCount || 3;

                root.editShowUndescribedBinds = defaults.showUndescribedBinds ?? false;

                root.editUseMacSymbol = defaults.useMacSymbol ?? false;
                root.editUseFnSymbol = defaults.useFnSymbol ?? false;
                root.editUseMouseSymbol = defaults.useMouseSymbol ?? false;
                root.editSuperKeyText = defaults.superKeyText ?? "";
                root.editSplitButtons = defaults.splitButtons ?? true;

                widthInput.text = root.editWindowWidth.toString();
                heightInput.text = root.editWindowHeight.toString();


                if (pluginApi && pluginApi.pluginSettings) {
                  pluginApi.pluginSettings.cheatsheetData = [];
                  pluginApi.saveSettings();
                }

                ToastService.showNotice(pluginApi?.tr("settings.reset-message"));
              }
            }
          }
        }
      }

      // ===== Symbol Display =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: symbolContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: symbolContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginS

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.symbol-display")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.symbol-display-hint")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeS
            wrapMode: Text.WordWrap
          }

          NToggle {
            Layout.fillWidth: true
            label: pluginApi?.tr("settings.use-mac-symbol")
            description: pluginApi?.tr("settings.use-mac-symbol-hint")
            checked: root.editUseMacSymbol
            onToggled: function(checked) {
              root.editUseMacSymbol = checked;
            }
          }

          NToggle {
            Layout.fillWidth: true
            label: pluginApi?.tr("settings.use-fn-symbol")
            description: pluginApi?.tr("settings.use-fn-symbol-hint")
            checked: root.editUseFnSymbol
            onToggled: function(checked) {
              root.editUseFnSymbol = checked;
            }
          }

          NToggle {
            Layout.fillWidth: true
            label: pluginApi?.tr("settings.use-mouse-symbol")
            description: pluginApi?.tr("settings.use-mouse-symbol-hint")
            checked: root.editUseMouseSymbol
            onToggled: function(checked) {
              root.editUseMouseSymbol = checked;
            }
          }

          RowLayout {
            spacing: Style.marginM

            NText {
              text: pluginApi?.tr("settings.super-key-text")
              color: Color.mOnSurface
              pointSize: Style.fontSizeM
              Layout.fillWidth: true
            }

            NTextInput {
              Layout.preferredWidth: 120 * Style.uiScaleRatio
              Layout.preferredHeight: Style.baseWidgetSize
              text: root.editSuperKeyText
              placeholderText: "\uE8E5"
              fontFamily: Settings.data.ui.fontFixed
              onTextChanged: {
                root.editSuperKeyText = text;
              }
            }
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.super-key-text-hint")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeXS
            wrapMode: Text.WordWrap
          }
        }
      }

      // ===== Keybind Setup =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: keybindContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: keybindContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginS

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.keybind-setup")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.keybind-setup-description")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeS
            wrapMode: Text.WordWrap
          }

          Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: commandText.implicitHeight + Style.marginS * 2
            color: Color.mSurface
            radius: Style.radiusS

            NText {
              id: commandText
              anchors.fill: parent
              anchors.margins: Style.marginS
              text: pluginApi?.tr("settings.keybind-ipc-command")
              font.family: "monospace"
              pointSize: Style.fontSizeS
              color: Color.mPrimary
              wrapMode: Text.WrapAnywhere
            }
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.keybind-example-hyprland")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeXS
            wrapMode: Text.WordWrap
          }


        }
      }

      Item {
        Layout.preferredHeight: Style.marginL
      }
    }
    // ============ END TAB 1 ============

    // ============ TAB 2: APPEARANCE ============
    ColumnLayout {
      id: appearanceTab
      spacing: Style.marginM

      // Header
      NText {
        Layout.fillWidth: true
        text: pluginApi?.tr("settings.appearance-section")
        pointSize: Style.fontSizeXXL
        font.weight: Style.fontWeightBold
        color: Color.mOnSurface
      }

      NText {
        Layout.fillWidth: true
        text: pluginApi?.tr("settings.appearance-description")
        color: Color.mOnSurfaceVariant
        pointSize: Style.fontSizeM
        wrapMode: Text.WordWrap
      }

      // ===== Live Preview =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: previewContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: previewContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginM

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.preview-section")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          Flow {
            Layout.fillWidth: true
            spacing: Style.marginS

            Component {
              id: previewBadge
              Rectangle {
                property string label: ""
                property color bg: "#888888"
                property color fg: "#FFFFFF"
                width: badgeText.implicitWidth + Style.marginM * 2
                height: badgeText.implicitHeight + Style.marginS * 2
                radius: Style.radiusS
                color: bg
                border.color: Qt.darker(bg, 1.4)
                border.width: Math.max(1, Style.borderS)

                NText {
                  id: badgeText
                  anchors.centerIn: parent
                  text: parent.label
                  color: parent.fg
                  pointSize: Style.fontSizeM
                  font.weight: Style.fontWeightBold
                }
              }
            }

            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Super";
              item.bg = Qt.binding(function() { return root.valueKeyColorSuper || Color.mPrimary });
              item.fg = Qt.binding(function() { return root.valueKeyTextSuper || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Ctrl";
              item.bg = Qt.binding(function() { return root.valueKeyColorCtrl || Color.mSecondary });
              item.fg = Qt.binding(function() { return root.valueKeyTextCtrl || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Shift";
              item.bg = Qt.binding(function() { return root.valueKeyColorShift || Color.mTertiary });
              item.fg = Qt.binding(function() { return root.valueKeyTextShift || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Alt";
              item.bg = Qt.binding(function() { return root.valueKeyColorAlt });
              item.fg = Qt.binding(function() { return root.valueKeyTextAlt || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "XF86Audio";
              item.bg = Qt.binding(function() { return root.valueKeyColorXF86 });
              item.fg = Qt.binding(function() { return root.valueKeyTextXF86 || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Print";
              item.bg = Qt.binding(function() { return root.valueKeyColorPrint });
              item.fg = Qt.binding(function() { return root.valueKeyTextPrint || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "1-9";
              item.bg = Qt.binding(function() { return root.valueKeyColorNumeric });
              item.fg = Qt.binding(function() { return root.valueKeyTextNumeric || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "mouse:272";
              item.bg = Qt.binding(function() { return root.valueKeyColorMouse });
              item.fg = Qt.binding(function() { return root.valueKeyTextMouse || root.valueKeyLabelColor });
            } }
            Loader { sourceComponent: previewBadge; onLoaded: {
              item.label = "Q";
              item.bg = Qt.binding(function() { return root.valueKeyColorDefault });
              item.fg = Qt.binding(function() { return root.valueKeyTextDefault || root.valueKeyLabelColor });
            } }
          }

          // Description preview sample
          NText {
            Layout.fillWidth: true
            Layout.topMargin: Style.marginS
            text: pluginApi?.tr("settings.color-description") + " — sample"
            color: root.valueDescriptionTextColor
            pointSize: Style.fontSizeM
          }
        }
      }

      // ===== Color Pickers =====
      NBox {
        Layout.fillWidth: true
        Layout.preferredHeight: colorContent.implicitHeight + Style.marginM * 2
        color: Color.mSurfaceVariant

        ColumnLayout {
          id: colorContent
          anchors.fill: parent
          anchors.margins: Style.marginM
          spacing: Style.marginM

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.color-pickers")
            pointSize: Style.fontSizeL
            font.weight: Style.fontWeightBold
            color: Color.mOnSurface
          }

          NText {
            Layout.fillWidth: true
            text: pluginApi?.tr("settings.color-theme-hint")
            color: Color.mOnSurfaceVariant
            pointSize: Style.fontSizeS
            wrapMode: Text.WordWrap
          }

          RowLayout {
            Layout.fillWidth: true
            spacing: Style.marginS
            NIcon {
              icon: "clipboard"
              pointSize: Style.fontSizeM
              color: root.clipboardHex.length > 0 ? Color.mPrimary : Color.mOnSurfaceVariant
            }
            NText {
              Layout.fillWidth: true
              text: pluginApi?.tr("settings.color-paste-hint")
              color: Color.mOnSurfaceVariant
              pointSize: Style.fontSizeS
              wrapMode: Text.WordWrap
            }
          }

          // Super (modifier: empty bg = theme accent)
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-super")
            letter: "S"
            bgValue: root.valueKeyColorSuper
            textValue: root.valueKeyTextSuper
            bgFallback: Color.mPrimary
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorSuper = c.toString(); root._applyPreview("keyColorSuper", c.toString()); }
            onTextPicked: c => { root.valueKeyTextSuper = c.toString(); root._applyPreview("keyTextSuper", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorSuper = hex; root._applyPreview("keyColorSuper", hex); }
            onTextPasted: hex => { root.valueKeyTextSuper = hex; root._applyPreview("keyTextSuper", hex); }
            onResetRequested: {
              root.valueKeyColorSuper = ""; root._applyPreview("keyColorSuper", "");
              root.valueKeyTextSuper  = ""; root._applyPreview("keyTextSuper", "");
            }
          }

          // Ctrl
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-ctrl")
            letter: "C"
            bgValue: root.valueKeyColorCtrl
            textValue: root.valueKeyTextCtrl
            bgFallback: Color.mSecondary
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorCtrl = c.toString(); root._applyPreview("keyColorCtrl", c.toString()); }
            onTextPicked: c => { root.valueKeyTextCtrl = c.toString(); root._applyPreview("keyTextCtrl", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorCtrl = hex; root._applyPreview("keyColorCtrl", hex); }
            onTextPasted: hex => { root.valueKeyTextCtrl = hex; root._applyPreview("keyTextCtrl", hex); }
            onResetRequested: {
              root.valueKeyColorCtrl = ""; root._applyPreview("keyColorCtrl", "");
              root.valueKeyTextCtrl  = ""; root._applyPreview("keyTextCtrl", "");
            }
          }

          // Shift
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-shift")
            letter: "⇧"
            bgValue: root.valueKeyColorShift
            textValue: root.valueKeyTextShift
            bgFallback: Color.mTertiary
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorShift = c.toString(); root._applyPreview("keyColorShift", c.toString()); }
            onTextPicked: c => { root.valueKeyTextShift = c.toString(); root._applyPreview("keyTextShift", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorShift = hex; root._applyPreview("keyColorShift", hex); }
            onTextPasted: hex => { root.valueKeyTextShift = hex; root._applyPreview("keyTextShift", hex); }
            onResetRequested: {
              root.valueKeyColorShift = ""; root._applyPreview("keyColorShift", "");
              root.valueKeyTextShift  = ""; root._applyPreview("keyTextShift", "");
            }
          }

          // Alt
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-alt")
            letter: "A"
            bgValue: root.valueKeyColorAlt
            textValue: root.valueKeyTextAlt
            bgFallback: defaults.keyColorAlt || "#FF6B6B"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorAlt = c.toString(); root._applyPreview("keyColorAlt", c.toString()); }
            onTextPicked: c => { root.valueKeyTextAlt = c.toString(); root._applyPreview("keyTextAlt", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorAlt = hex; root._applyPreview("keyColorAlt", hex); }
            onTextPasted: hex => { root.valueKeyTextAlt = hex; root._applyPreview("keyTextAlt", hex); }
            onResetRequested: {
              var d = defaults.keyColorAlt || "#FF6B6B";
              root.valueKeyColorAlt = d; root._applyPreview("keyColorAlt", d);
              root.valueKeyTextAlt  = ""; root._applyPreview("keyTextAlt", "");
            }
          }

          // XF86
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-xf86")
            letter: "♪"
            bgValue: root.valueKeyColorXF86
            textValue: root.valueKeyTextXF86
            bgFallback: defaults.keyColorXF86 || "#4ECDC4"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorXF86 = c.toString(); root._applyPreview("keyColorXF86", c.toString()); }
            onTextPicked: c => { root.valueKeyTextXF86 = c.toString(); root._applyPreview("keyTextXF86", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorXF86 = hex; root._applyPreview("keyColorXF86", hex); }
            onTextPasted: hex => { root.valueKeyTextXF86 = hex; root._applyPreview("keyTextXF86", hex); }
            onResetRequested: {
              var d = defaults.keyColorXF86 || "#4ECDC4";
              root.valueKeyColorXF86 = d; root._applyPreview("keyColorXF86", d);
              root.valueKeyTextXF86  = ""; root._applyPreview("keyTextXF86", "");
            }
          }

          // Print
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-print")
            letter: "P"
            bgValue: root.valueKeyColorPrint
            textValue: root.valueKeyTextPrint
            bgFallback: defaults.keyColorPrint || "#95E1D3"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorPrint = c.toString(); root._applyPreview("keyColorPrint", c.toString()); }
            onTextPicked: c => { root.valueKeyTextPrint = c.toString(); root._applyPreview("keyTextPrint", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorPrint = hex; root._applyPreview("keyColorPrint", hex); }
            onTextPasted: hex => { root.valueKeyTextPrint = hex; root._applyPreview("keyTextPrint", hex); }
            onResetRequested: {
              var d = defaults.keyColorPrint || "#95E1D3";
              root.valueKeyColorPrint = d; root._applyPreview("keyColorPrint", d);
              root.valueKeyTextPrint  = ""; root._applyPreview("keyTextPrint", "");
            }
          }

          // Numeric
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-numeric")
            letter: "1"
            bgValue: root.valueKeyColorNumeric
            textValue: root.valueKeyTextNumeric
            bgFallback: defaults.keyColorNumeric || "#A8DADC"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorNumeric = c.toString(); root._applyPreview("keyColorNumeric", c.toString()); }
            onTextPicked: c => { root.valueKeyTextNumeric = c.toString(); root._applyPreview("keyTextNumeric", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorNumeric = hex; root._applyPreview("keyColorNumeric", hex); }
            onTextPasted: hex => { root.valueKeyTextNumeric = hex; root._applyPreview("keyTextNumeric", hex); }
            onResetRequested: {
              var d = defaults.keyColorNumeric || "#A8DADC";
              root.valueKeyColorNumeric = d; root._applyPreview("keyColorNumeric", d);
              root.valueKeyTextNumeric  = ""; root._applyPreview("keyTextNumeric", "");
            }
          }

          // Mouse
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-mouse")
            letter: "M"
            bgValue: root.valueKeyColorMouse
            textValue: root.valueKeyTextMouse
            bgFallback: defaults.keyColorMouse || "#F38181"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorMouse = c.toString(); root._applyPreview("keyColorMouse", c.toString()); }
            onTextPicked: c => { root.valueKeyTextMouse = c.toString(); root._applyPreview("keyTextMouse", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorMouse = hex; root._applyPreview("keyColorMouse", hex); }
            onTextPasted: hex => { root.valueKeyTextMouse = hex; root._applyPreview("keyTextMouse", hex); }
            onResetRequested: {
              var d = defaults.keyColorMouse || "#F38181";
              root.valueKeyColorMouse = d; root._applyPreview("keyColorMouse", d);
              root.valueKeyTextMouse  = ""; root._applyPreview("keyTextMouse", "");
            }
          }

          // Default letter keys
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-default")
            letter: "Q"
            bgValue: root.valueKeyColorDefault
            textValue: root.valueKeyTextDefault
            bgFallback: defaults.keyColorDefault || "#6C757D"
            textFallback: root.valueKeyLabelColor
            clipboardHex: root.clipboardHex
            onBgPicked: c => { root.valueKeyColorDefault = c.toString(); root._applyPreview("keyColorDefault", c.toString()); }
            onTextPicked: c => { root.valueKeyTextDefault = c.toString(); root._applyPreview("keyTextDefault", c.toString()); }
            onBgPasted: hex => { root.valueKeyColorDefault = hex; root._applyPreview("keyColorDefault", hex); }
            onTextPasted: hex => { root.valueKeyTextDefault = hex; root._applyPreview("keyTextDefault", hex); }
            onResetRequested: {
              var d = defaults.keyColorDefault || "#6C757D";
              root.valueKeyColorDefault = d; root._applyPreview("keyColorDefault", d);
              root.valueKeyTextDefault  = ""; root._applyPreview("keyTextDefault", "");
            }
          }

          // Description text (text-only row — no background)
          Local.ColorPairRow {
            pluginApi: root.pluginApi
            labelText: pluginApi?.tr("settings.color-description")
            letter: "T"
            showBg: false
            bgValue: ""
            textValue: root.valueDescriptionTextColor
            textFallback: defaults.descriptionTextColor || "#E0E0E0"
            clipboardHex: root.clipboardHex
            onTextPicked: c => { root.valueDescriptionTextColor = c.toString(); root._applyPreview("descriptionTextColor", c.toString()); }
            onTextPasted: hex => { root.valueDescriptionTextColor = hex; root._applyPreview("descriptionTextColor", hex); }
            onResetRequested: {
              var d = defaults.descriptionTextColor || "#E0E0E0";
              root.valueDescriptionTextColor = d; root._applyPreview("descriptionTextColor", d);
            }
          }

          Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Color.mOutline
            opacity: 0.3
          }

          NButton {
            Layout.alignment: Qt.AlignRight
            text: pluginApi?.tr("settings.reset-all-colors")
            icon: "rotate"
            onClicked: {
              var dAlt     = defaults.keyColorAlt     || "#FF6B6B";
              var dXF86    = defaults.keyColorXF86    || "#4ECDC4";
              var dPrint   = defaults.keyColorPrint   || "#95E1D3";
              var dNumeric = defaults.keyColorNumeric || "#A8DADC";
              var dMouse   = defaults.keyColorMouse   || "#F38181";
              var dDefault = defaults.keyColorDefault || "#6C757D";
              var dLabel   = defaults.keyLabelColor   || "#FFFFFF";
              var dDesc    = defaults.descriptionTextColor || "#E0E0E0";

              root.valueKeyColorSuper = "";
              root.valueKeyColorCtrl = "";
              root.valueKeyColorShift = "";
              root.valueKeyColorAlt = dAlt;
              root.valueKeyColorXF86 = dXF86;
              root.valueKeyColorPrint = dPrint;
              root.valueKeyColorNumeric = dNumeric;
              root.valueKeyColorMouse = dMouse;
              root.valueKeyColorDefault = dDefault;
              root.valueKeyLabelColor = dLabel;
              root.valueDescriptionTextColor = dDesc;

              root.valueKeyTextSuper = "";
              root.valueKeyTextCtrl = "";
              root.valueKeyTextShift = "";
              root.valueKeyTextAlt = "";
              root.valueKeyTextXF86 = "";
              root.valueKeyTextPrint = "";
              root.valueKeyTextNumeric = "";
              root.valueKeyTextMouse = "";
              root.valueKeyTextDefault = "";

              if (pluginApi) {
                pluginApi.pluginSettings = Object.assign({}, pluginApi.pluginSettings, {
                  keyColorSuper: "",
                  keyColorCtrl: "",
                  keyColorShift: "",
                  keyColorAlt: dAlt,
                  keyColorXF86: dXF86,
                  keyColorPrint: dPrint,
                  keyColorNumeric: dNumeric,
                  keyColorMouse: dMouse,
                  keyColorDefault: dDefault,
                  keyLabelColor: dLabel,
                  descriptionTextColor: dDesc,
                  keyTextSuper: "",
                  keyTextCtrl: "",
                  keyTextShift: "",
                  keyTextAlt: "",
                  keyTextXF86: "",
                  keyTextPrint: "",
                  keyTextNumeric: "",
                  keyTextMouse: "",
                  keyTextDefault: ""
                });
              }
            }
          }
        }
      }

      Item {
        Layout.preferredHeight: Style.marginL
      }
    }
    // ============ END TAB 2 ============
  }
  // ============ END TAB VIEW ============

  // Save function called by the shell when user clicks Apply/Save
  function saveSettings() {
    if (!pluginApi) return;

    pluginApi.pluginSettings.windowWidth = root.editWindowWidth;
    pluginApi.pluginSettings.windowHeight = root.editWindowHeight;
    pluginApi.pluginSettings.autoHeight = root.editAutoHeight;
    pluginApi.pluginSettings.columnCount = root.editColumnCount;

    pluginApi.pluginSettings.showUndescribedBinds = root.editShowUndescribedBinds;

    pluginApi.pluginSettings.useMacSymbol = root.editUseMacSymbol;
    pluginApi.pluginSettings.useFnSymbol = root.editUseFnSymbol;
    pluginApi.pluginSettings.useMouseSymbol = root.editUseMouseSymbol;
    pluginApi.pluginSettings.superKeyText = root.editSuperKeyText;
    pluginApi.pluginSettings.splitButtons = root.editSplitButtons;

    pluginApi.pluginSettings.keyColorAlt = root.valueKeyColorAlt.toString();
    pluginApi.pluginSettings.keyColorXF86 = root.valueKeyColorXF86.toString();
    pluginApi.pluginSettings.keyColorPrint = root.valueKeyColorPrint.toString();
    pluginApi.pluginSettings.keyColorNumeric = root.valueKeyColorNumeric.toString();
    pluginApi.pluginSettings.keyColorMouse = root.valueKeyColorMouse.toString();
    pluginApi.pluginSettings.keyColorSuper = root.valueKeyColorSuper;
    pluginApi.pluginSettings.keyColorCtrl  = root.valueKeyColorCtrl;
    pluginApi.pluginSettings.keyColorShift = root.valueKeyColorShift;
    pluginApi.pluginSettings.keyColorDefault = root.valueKeyColorDefault.toString();
    pluginApi.pluginSettings.keyLabelColor = root.valueKeyLabelColor.toString();
    pluginApi.pluginSettings.descriptionTextColor = root.valueDescriptionTextColor.toString();

    pluginApi.pluginSettings.keyTextSuper   = root.valueKeyTextSuper;
    pluginApi.pluginSettings.keyTextCtrl    = root.valueKeyTextCtrl;
    pluginApi.pluginSettings.keyTextShift   = root.valueKeyTextShift;
    pluginApi.pluginSettings.keyTextAlt     = root.valueKeyTextAlt;
    pluginApi.pluginSettings.keyTextXF86    = root.valueKeyTextXF86;
    pluginApi.pluginSettings.keyTextPrint   = root.valueKeyTextPrint;
    pluginApi.pluginSettings.keyTextNumeric = root.valueKeyTextNumeric;
    pluginApi.pluginSettings.keyTextMouse   = root.valueKeyTextMouse;
    pluginApi.pluginSettings.keyTextDefault = root.valueKeyTextDefault;

    _applied = true;
    pluginApi.saveSettings();

    if (pluginApi.mainInstance) {
      pluginApi.mainInstance.refresh();
    }

    ToastService.showNotice(pluginApi?.tr("settings.saved"));
  }
}
