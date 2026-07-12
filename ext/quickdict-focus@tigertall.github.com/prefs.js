import Gio from 'gi://Gio';
import Adw from 'gi://Adw';
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

        window.add(page);
    }
}
