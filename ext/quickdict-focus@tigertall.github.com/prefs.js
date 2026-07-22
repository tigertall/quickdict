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

        window.add(page);
    }
}
