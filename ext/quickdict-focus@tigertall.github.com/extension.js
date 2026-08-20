import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Pango from 'gi://Pango';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const DBUS_FACE = 'io.github.tigertall.QuickDict.Translator';
const DBUS_PATH = '/io/github/tigertall/QuickDict/Translator';
const APP_ID = 'io.github.tigertall.QuickDict';

// 全局状态变量显式声明
let debounce = 0;
let lastWord = '';
let lastText = '';
let settings = null;
let panelButton = null;
let popup = null;
let activeSignals = [];
let leaveTimeoutId = 0;
const BUFFER_PX = 10;

// 悬停取词状态
let hoverPollId = 0;
let hoverTimerId = 0;
let lastHoverX = -1;
let lastHoverY = -1;
let isHoverPopupActive = false;

// 弹窗来源与异步竞态防护：划词（用户主动）优先于取词（被动）
let popupSource = null;      // 'hover' | 'selection' | null
let actionGeneration = 0;    // 每次用户动作递增，过期回调据此丢弃

function isSystemDark() {
    try {
        let interfaceSettings = new Gio.Settings({ schema_id: 'org.gnome.desktop.interface' });
        let scheme = interfaceSettings.get_string('color-scheme');
        return scheme === 'prefer-dark';
    } catch (e) {
        return false;
    }
}

function resolveTheme() {
    if (!settings) return 'dark';
    let theme = settings.get_string('popup-theme');
    if (theme === 'dark') return 'dark';
    if (theme === 'light') return 'light';
    // auto: follow system
    return isSystemDark() ? 'dark' : 'light';
}

function applyPopupTheme(box) {
    let theme = resolveTheme();
    if (theme === 'light') {
        box.style_class = 'quickdict-popup quickdict-popup-light';
    } else {
        box.style_class = 'quickdict-popup quickdict-popup-dark';
    }
}

function ensurePopup() {
    if (!popup) {
        popup = new St.BoxLayout({
            vertical: true,
            reactive: true,
            track_hover: true,
            x: 0, y: 0,
            visible: false,
            width: 400
        });
        applyPopupTheme(popup);
        Main.uiGroup.add_child(popup);
    }
    return popup;
}

function ungrab() {
    // 1. 安全销毁残留的延时定时器，防止定时器在后台残留导致意外关闭新弹窗
    if (leaveTimeoutId) {
        GLib.source_remove(leaveTimeoutId);
        leaveTimeoutId = 0;
    }
    // 2. 切断信号
    activeSignals.forEach(id => {
        if (popup) {
            try { popup.disconnect(id); } catch (e) {}
        }
    });
    activeSignals = [];
}

function closePopup() {
    if (!popup) return;
    popup.visible = false;
    popup.remove_all_children();
    ungrab();
    isHoverPopupActive = false;
    popupSource = null;
}

function showResults(jsonStr, titleWord) {
    const box = ensurePopup();
    
    closePopup();
    box.set_height(-1);

    let results = [];
    try { results = JSON.parse(jsonStr); } catch (e) {}
    if (!results || results.length === 0) return;

    // 单词标题（OCR 取词时显示识别出的单词）
    if (titleWord) {
        let theme = resolveTheme();
        let wClass = (theme === 'light') ? 'quickdict-word-light' : 'quickdict-word-dark';
        box.add_child(new St.Label({ text: titleWord, style_class: wClass }));
    }

    const scroll = new St.ScrollView({ hscrollbar_policy: St.PolicyType.NEVER, vscrollbar_policy: St.PolicyType.AUTOMATIC,
        x_expand: true, y_expand: true });
    scroll.set_style('min-height: 100px');
    const content = new St.BoxLayout({ vertical: true });
    
    for (const r of results) {
        let theme = resolveTheme();
        let dnClass = (theme === 'light') ? 'quickdict-dict-name-light' : 'quickdict-dict-name-dark';
        let txClass = (theme === 'light') ? 'quickdict-text-light' : 'quickdict-text-dark';
        content.add_child(new St.Label({ text: r.name, style_class: dnClass }));
        const t = new St.Label({ style_class: txClass });
        t.clutter_text.use_markup = true;
        t.clutter_text.set_markup(r.text || '');
        t.clutter_text.single_line_mode = false; t.clutter_text.line_wrap = true;
        t.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR; t.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        content.add_child(t);
    }
    scroll.set_child(content); 
    box.add_child(scroll);

    const btn = new St.Button({ style_class: 'button', label: 'Open in Dictionary', reactive: true, can_focus: true, x_expand: true });
    btn.connect('clicked', () => {
        const w = lastWord;
        let appPath = '/' + APP_ID.replace(/\./g, '/');
        let wv = new GLib.Variant('s', w);
        let body = new GLib.Variant('(sava{sv})', ['search-word', [wv], {}]);
        Gio.DBus.session.call(APP_ID, appPath, 'org.gtk.Actions', 'Activate', body,
            null, Gio.DBusCallFlags.NONE, -1, null, (conn, res) => {
                try { conn.call_finish(res); } catch (e) {}
            }
        );
        closePopup();
        try {
            const windowList = global.display.list_all_windows();
            for (const win of windowList) {
                if ((win.get_wm_class() || '').toLowerCase().includes('quickdict')) {
                    win.activate(global.display.get_current_time_roundtrip());
                    break;
                }
            }
        } catch (e) {}
    });
    box.add_child(btn);

    const [x, y] = global.get_pointer();
    const [, nh] = box.get_preferred_height(400);
    const bh = Math.min(nh, global.screen_height * 0.5);
    box.set_height(bh);
    
    let py = y - 2;
    if (y > global.screen_height * 0.55) py = y - bh + 2;
    box.x = Math.max(10, Math.min(x - 2, global.screen_width - 410));
    box.y = Math.max(10, Math.min(py, global.screen_height - bh - 10));
    box.visible = true;

    activeSignals.push(box.connect('enter-event', () => {
        isHoverPopupActive = true;
        if (leaveTimeoutId) {
            // log('[QuickDict] 鼠标在 200ms 内重返弹窗内部，成功拦截并取消退场定时器');
            GLib.source_remove(leaveTimeoutId);
            leaveTimeoutId = 0;
        }
    }));

    activeSignals.push(box.connect('leave-event', () => {
        // 先清理可能残余的旧定时器，防止多重计时冲突
        if (leaveTimeoutId) {
            GLib.source_remove(leaveTimeoutId);
            leaveTimeoutId = 0;
        }

        // 注册一个 200 毫秒后执行的宏任务
        leaveTimeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
            leaveTimeoutId = 0; // 执行时清空 ID
            
            // 200ms 时间到，执行二次物理平面判定
            const [mx, my] = global.get_pointer();
            const [bx, by] = box.get_transformed_position();
            const [bw, bhv] = box.get_transformed_size();

            
            const isReallyOutside = (
                mx < (bx - BUFFER_PX) || 
                mx > (bx + bw + BUFFER_PX) || 
                my < (by - BUFFER_PX) || 
                my > (by + bhv + BUFFER_PX)
            );

            if (isReallyOutside) {
                //log('[QuickDict] 200ms 观察期结束，鼠标依然在外部，弹窗默默收起');
                closePopup();
            } else {
                //log('[QuickDict] 200ms 观察期结束，鼠标已返回内部，取消关闭操作');
            }

            return GLib.SOURCE_REMOVE; // 销毁定时器自身
        });
    }));
}

function isAppAllowed() {
    if (!settings) return true;
    let filter = settings.get_string('app-filter').trim();
    if (!filter) return true;
    let allowed = filter.split(',').map(s => s.trim().toLowerCase());
    let win = global.display.get_focus_window();
    let wmClass = (win ? win.get_wm_class() || '' : '').toLowerCase();
    let ok = allowed.some(a => wmClass.includes(a));
    return ok;
}

function isModifierActive(mods, modifierKey) {
    if (modifierKey === 'none') return true;
    switch (modifierKey) {
        case 'Ctrl': return !!(mods & Clutter.ModifierType.CONTROL_MASK);
        case 'Alt': return !!(mods & Clutter.ModifierType.MOD1_MASK);
        case 'Shift': return !!(mods & Clutter.ModifierType.SHIFT_MASK);
        case 'Super': return !!(mods & Clutter.ModifierType.MOD4_MASK);
        default: return true;
    }
}

// --- OCR capture (replaces AT-SPI word extraction) ---
// Use Shell.Screenshot (libshell GI) directly: the extension runs inside
// the GNOME Shell process, so we can capture compositor frames without
// going through D-Bus. The org.gnome.Shell.Screenshot D-Bus service
// restricts callers to a whitelist since GNOME 49 (SettingsDaemon
// MediaKeys and the portal backend), and a synchronous D-Bus call to
// the shell's own service would deadlock the main loop.
const CAPTURE_W = 320;
const CAPTURE_H = 64;
const OCR_DEBUG_DIR = '/tmp/quickdict-ocr';
let lastOcrX = -1;
let lastOcrY = -1;
let ocrInFlight = false;

// Save captured PNG to the debug dir for inspection (non-fatal on failure)
function saveOcrDebugPng(png, sx, sy) {
    try {
        let dir = Gio.File.new_for_path(OCR_DEBUG_DIR);
        if (!dir.query_exists(null))
            dir.make_directory_with_parents(null);
        let stamp = new Date().toISOString().replace(/[:.]/g, '-');
        let path = OCR_DEBUG_DIR + '/ocr-' + stamp + '-' + sx + '-' + sy + '.png';
        GLib.file_set_contents(path, png);
        log('[QuickDict] OCR: saved debug capture to ' + path);
    } catch (e) {
        log('[QuickDict] OCR: failed to save debug capture: ' + e);
    }
}

// Async capture of the screen region (sx, sy, CAPTURE_W, CAPTURE_H) as PNG bytes
function captureRegionPng(sx, sy) {
    return new Promise((resolve, reject) => {
        try {
            let shooter = new Shell.Screenshot();
            let stream = Gio.MemoryOutputStream.new_resizable();
            shooter.screenshot_area(sx, sy, CAPTURE_W, CAPTURE_H, stream)
                .then(() => {
                    // steal_as_bytes() requires the stream to be closed first
                    stream.close(null);
                    resolve(stream.steal_as_bytes().toArray());
                })
                .catch(e => {
                    reject(e);
                });
        } catch (e) {
            reject(e);
        }
    });
}

// Debug-level log: only printed when debug-log is enabled (default off)
function debugLog(msg) {
    if (settings && settings.get_boolean('debug-log')) log(msg);
}

function triggerHoverLookup(x, y) {
    // 划词弹窗存活期间，被动取词静默（用户主动选择优先）
    if (popupSource === 'selection') return;
    // Position-based dedup: same spot within 5px already looked up
    if (Math.abs(x - lastOcrX) < 5 && Math.abs(y - lastOcrY) < 5) return;
    if (ocrInFlight) return;
    lastOcrX = x;
    lastOcrY = y;
    ocrInFlight = true;

    // Clamp capture region to screen bounds
    let sx = Math.max(0, Math.min(x - Math.floor(CAPTURE_W / 2), global.screen_width - CAPTURE_W));
    let sy = Math.max(0, Math.min(y - Math.floor(CAPTURE_H / 2), global.screen_height - CAPTURE_H));

    captureRegionPng(sx, sy).then(png => {
        // 调试截图保存由 GSettings 开关控制（默认关闭）
        if (settings && settings.get_boolean('debug-save-capture')) {
            saveOcrDebugPng(png, sx, sy);
        }
        let b64 = GLib.base64_encode(png);
        // Cursor offset relative to region top-left
        let cx = x - sx;
        let cy = y - sy;
        debugLog('[QuickDict] OCR: captured ' + CAPTURE_W + 'x' + CAPTURE_H + ' at (' + sx + ',' + sy + ') cursor offset (' + cx + ',' + cy + ')');
        const myGen = actionGeneration;
        Gio.DBus.session.call(DBUS_FACE, DBUS_PATH, DBUS_FACE, 'LookupImage',
            GLib.Variant.new('(siiii)', [b64, cx, cy, CAPTURE_W, CAPTURE_H]), null, Gio.DBusCallFlags.NONE, 10000, null,
            (_conn, res) => {
                try {
                    if (myGen !== actionGeneration) return; // 用户已划词，丢弃过期取词结果
                    let data = Gio.DBus.session.call_finish(res).deepUnpack()[0];
                    // LookupImage 返回 {"word": ..., "results": [...]}
                    let resp = JSON.parse(data);
                    let results = (resp && resp.results) || [];
                    let word = (resp && resp.word) || null;
                    if (!results || results.length === 0) {
                        debugLog('[QuickDict] OCR: no results (empty response)');
                        closePopup();
                        return;
                    }
                    debugLog('[QuickDict] OCR: got ' + results.length + ' result(s), showing popup');
                    if (word) lastWord = word;
                    showResults(JSON.stringify(results), word);
                    popupSource = 'hover';
                    isHoverPopupActive = true;
                } catch (e) {
                    if (myGen !== actionGeneration) return;
                    log('[QuickDict] OCR: LookupImage failed: ' + e);
                    closePopup();
                }
            }
        );
    }).catch(e => {
        log('[QuickDict] OCR: capture failed: ' + e);
    }).finally(() => {
        ocrInFlight = false;
    });
}

function pollHover() {
    if (!settings || !settings.get_boolean('hover-monitor')) return;
    if (isHoverPopupActive) return;
    // 划词弹窗存活期间，被动取词静默
    if (popupSource === 'selection') return;
    let pointer = global.get_pointer();
    let x = pointer[0];
    let y = pointer[1];
    let mods = pointer.length > 2 ? pointer[2] : 0;
    if (x === lastHoverX && y === lastHoverY) return;
    lastHoverX = x;
    lastHoverY = y;
    if (hoverTimerId) {
        GLib.source_remove(hoverTimerId);
        hoverTimerId = 0;
    }
    let modifierKey = settings.get_string('hover-modifier');
    if (!isModifierActive(mods, modifierKey)) return;
    if (!isAppAllowed()) return;
    let delay = settings.get_int('hover-delay');
    hoverTimerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
        hoverTimerId = 0;
        triggerHoverLookup(lastHoverX, lastHoverY);
        return GLib.SOURCE_REMOVE;
    });
}

function startHoverPolling() {
    if (hoverPollId) return;
    log('[QuickDict] hover polling started (50ms interval)');
    hoverPollId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
        pollHover();
        return GLib.SOURCE_CONTINUE;
    });
}

function stopHoverPolling() {
    if (hoverPollId) {
        log('[QuickDict] hover polling stopped');
        GLib.source_remove(hoverPollId);
        hoverPollId = 0;
    }
    if (hoverTimerId) {
        GLib.source_remove(hoverTimerId);
        hoverTimerId = 0;
    }
}

export default class QuickDictFocusExtension extends Extension {
    _errorId = 0;
    _settingsId = null;
    _themeId = null;
    _systemSettings = null;
    _systemThemeId = null;
    _styleSheet = null;
    _selId = null;
    _hoverMonitorId = null;

    enable() {
        settings = this.getSettings();
        log('[QuickDict]: focus extension enabled');

        panelButton = new PanelMenu.Button(0.5, 'QuickDict', false);
        let icon = new St.Icon({ icon_name: 'accessories-dictionary-symbolic', style_class: 'system-status-icon' });
        panelButton.add_child(icon);

        // 任一功能启用即全亮，全部禁用才变暗
        let updateIcon = () => {
            if (this._errorId) { GLib.source_remove(this._errorId); this._errorId = 0; }
            icon.set_style(null);
            let anyEnabled = settings.get_boolean('clipboard-monitor') || settings.get_boolean('hover-monitor');
            icon.opacity = anyEnabled ? 255 : 100;
        };

        let showError = () => {
            if (this._errorId) GLib.source_remove(this._errorId);
            icon.set_style('color: #e01b24;');
            this._errorId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1500, () => { updateIcon(); this._errorId = 0; return GLib.SOURCE_REMOVE; });
        };
        updateIcon();

        let monitorItem = new PopupMenu.PopupSwitchMenuItem('Clipboard Monitor', settings.get_boolean('clipboard-monitor'));
        monitorItem.connect('toggled', (item, state) => { settings.set_boolean('clipboard-monitor', state); updateIcon(); });
        panelButton.menu.addMenuItem(monitorItem);
        this._settingsId = settings.connect('changed::clipboard-monitor', () => { monitorItem.setToggleState(settings.get_boolean('clipboard-monitor')); updateIcon(); });

        // 悬停取词开关（与 Clipboard Monitor 同级别）
        let hoverItem = new PopupMenu.PopupSwitchMenuItem('Hover Translation', settings.get_boolean('hover-monitor'));
        hoverItem.connect('toggled', (item, state) => { settings.set_boolean('hover-monitor', state); updateIcon(); });
        panelButton.menu.addMenuItem(hoverItem);
        this._hoverMonitorId = settings.connect('changed::hover-monitor', () => {
            hoverItem.setToggleState(settings.get_boolean('hover-monitor'));
            updateIcon();
            if (settings.get_boolean('hover-monitor')) {
                startHoverPolling();
            } else {
                stopHoverPolling();
            }
        });
        if (settings.get_boolean('hover-monitor')) {
            startHoverPolling();
        }
        // Listen for popup theme changes and update popup style dynamically
        this._themeId = settings.connect('changed::popup-theme', () => {
            if (popup) {
                applyPopupTheme(popup);
            }
        });

        // 监听系统 color-scheme 变更，Auto 模式下实时更新弹窗主题
        this._systemSettings = new Gio.Settings({ schema_id: 'org.gnome.desktop.interface' });
        this._systemThemeId = this._systemSettings.connect('changed::color-scheme', () => {
            if (settings && settings.get_string('popup-theme') === 'auto' && popup) {
                applyPopupTheme(popup);
            }
        });

        // Load extension CSS stylesheet
        let theme = St.ThemeContext.get_for_stage(global.stage).get_theme();
        let cssPath = this.dir.get_path() + '/stylesheet.css';
        this._styleSheet = theme.load_stylesheet(Gio.File.new_for_path(cssPath));

        Main.panel.addToStatusArea('quickdict', panelButton);

        this._selId = global.display.get_selection().connect('owner-changed', (_sel, selType, _source) => {
            if (selType !== 0) return;
            if (!settings.get_boolean('clipboard-monitor')) return;
            if (!isAppAllowed()) return;
            if (debounce) GLib.source_remove(debounce);
            debounce = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
                debounce = 0;
                St.Clipboard.get_default().get_text(St.ClipboardType.PRIMARY, (_c, text) => {
                    if (!text || !text.trim()) {
                        // Selection cleared — click-away: close popup
                        if (popup && popup.visible) closePopup();
                        return;
                    }
                    const word = text.trim();
                    if (word.length < 2 || word.length > 1000) return;
                    if (text === lastText) return;
                    lastText = text; lastWord = word;
                    // 划词优先：立即抢占现有弹窗，代际递增使在飞的取词结果过期
                    actionGeneration++;
                    const myGen = actionGeneration;
                    if (popup && popup.visible) closePopup();
                    // 立即标记 selection，使查询在飞期间 hover 也保持静默
                    popupSource = 'selection';
                    Gio.DBus.session.call(DBUS_FACE, DBUS_PATH, DBUS_FACE, 'Lookup',
                        GLib.Variant.new('(s)', [word]), null, Gio.DBusCallFlags.NONE, 5000, null,
                        (_conn, res) => {
                            try {
                                if (myGen !== actionGeneration) return; // 已被更新的划词取代
                                let data = Gio.DBus.session.call_finish(res).deepUnpack()[0];
                                // Lookup 返回 {"word": cleaned, "results": [...]}
                                let resp = JSON.parse(data);
                                let results = (resp && resp.results) || [];
                                let cleanWord = (resp && resp.word) || null;
                                if (!results || results.length === 0) { closePopup(); return; }
                                if (cleanWord) lastWord = cleanWord;
                                showResults(JSON.stringify(results), cleanWord);
                                popupSource = 'selection';
                            }
                            catch (e) {
                                if (myGen !== actionGeneration) return;
                                showError();
                                closePopup();
                                log('[QuickDict] showResults Exception' + e);
                             }
                        }
                    );
                });
                return GLib.SOURCE_REMOVE;
            });
        });
    }

    disable() {
        closePopup();
        stopHoverPolling();
        if (this._errorId) { GLib.source_remove(this._errorId); this._errorId = 0; }
        if (settings) {
            if (this._settingsId) { settings.disconnect(this._settingsId); this._settingsId = null; }
            if (this._themeId) { settings.disconnect(this._themeId); this._themeId = null; }
            if (this._systemThemeId) {
                this._systemSettings.disconnect(this._systemThemeId);
                this._systemThemeId = null;
                this._systemSettings = null;
            }
            if (this._hoverMonitorId) { settings.disconnect(this._hoverMonitorId); this._hoverMonitorId = null; }
            settings = null;        }
        if (panelButton) { panelButton.destroy(); panelButton = null; }
        if (popup) { global.stage.remove_child(popup); popup.destroy(); popup = null; }
        if (debounce) { GLib.source_remove(debounce); debounce = 0; }
        if (this._selId) { global.display.get_selection().disconnect(this._selId); this._selId = null; }
        if (this._styleSheet) {
            try {
                St.ThemeContext.get_for_stage(global.stage).get_theme().unload_stylesheet(this._styleSheet);
            } catch (e) {}
            this._styleSheet = null;
        }
    }
}
