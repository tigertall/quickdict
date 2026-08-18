import Gio from 'gi://Gio';
import Adw from 'gi://Adw';
import Gtk from 'gi://Gtk';
import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class QuickDictPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        let settings = this.getSettings();
        let page = new Adw.PreferencesPage();

        let filterGroup = new Adw.PreferencesGroup({ title: 'App Filter' });
        filterGroup.set_description('Only trigger for these App IDs (comma-separated). Empty = all. Examples: papers, foliate, firefox');
        let filterRow = new Adw.EntryRow({ title: 'App Filter' });
        settings.bind('app-filter', filterRow, 'text', Gio.SettingsBindFlags.DEFAULT);
        filterGroup.add(filterRow);
        page.add(filterGroup);

        let themeGroup = new Adw.PreferencesGroup({ title: 'Popup Theme' });
        themeGroup.set_description('Choose the color theme for the lookup popup.');
        let themeRow = new Adw.ComboRow({ title: 'Theme' });
        let model = new Gtk.StringList();
        model.append('Auto (Follow System)');
        model.append('Dark');
        model.append('Light');
        themeRow.set_model(model);
        let currentTheme = settings.get_string('popup-theme');
        if (currentTheme === 'dark') {
            themeRow.set_selected(1);
        } else if (currentTheme === 'light') {
            themeRow.set_selected(2);
        } else {
            themeRow.set_selected(0);
        }
        themeRow.connect('notify::selected', (row) => {
            let idx = row.selected;
            if (idx === 1) {
                settings.set_string('popup-theme', 'dark');
            } else if (idx === 2) {
                settings.set_string('popup-theme', 'light');
            } else {
                settings.set_string('popup-theme', 'auto');
            }
        });
        themeGroup.add(themeRow);
        page.add(themeGroup);

        // Hover Translation (enabled/disabled via the panel menu switch)
        let hoverGroup = new Adw.PreferencesGroup({ title: 'Hover Translation' });
        hoverGroup.set_description('Toggle via the panel menu switch. Hover over a word to translate via OCR.');

        let delayRow = new Adw.SpinRow({
            title: 'Hover Delay',
            subtitle: 'Milliseconds before triggering (200-2000)',
            adjustment: new Gtk.Adjustment({ lower: 200, upper: 2000, step_increment: 100 }),
            value: settings.get_int('hover-delay')
        });
        settings.bind('hover-delay', delayRow, 'value', Gio.SettingsBindFlags.DEFAULT);
        hoverGroup.add(delayRow);

        let modifierRow = new Adw.ComboRow({ title: 'Modifier Key' });
        let modifierModel = new Gtk.StringList();
        modifierModel.append('None');
        modifierModel.append('Ctrl');
        modifierModel.append('Alt');
        modifierModel.append('Shift');
        modifierModel.append('Super');
        modifierRow.set_model(modifierModel);
        let keys = ['none', 'Ctrl', 'Alt', 'Shift', 'Super'];
        let currentMod = settings.get_string('hover-modifier');
        let modIdx = keys.indexOf(currentMod);
        modifierRow.set_selected(modIdx >= 0 ? modIdx : 0);
        modifierRow.connect('notify::selected', (row) => {
            settings.set_string('hover-modifier', keys[row.selected] || 'none');
        });
        hoverGroup.add(modifierRow);

        page.add(hoverGroup);

        window.add(page);
    }
}
