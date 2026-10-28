ObjC.import('AppKit');

function session() {
    const processes = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('net.imput.helium');
    if (Number(processes.count) === 0) return '';
    const process = processes.objectAtIndex(0);
    return String(Number(process.processIdentifier)) + ':' + String(Number(process.launchDate.timeIntervalSince1970));
}

function fail(message) {
    const error = new Error(message);
    error.helMessage = message;
    throw error;
}

function identifier(value) {
    const number = Number(value);
    if (!Number.isSafeInteger(number) || number <= 0) fail('Helium returned an invalid identifier');
    return number;
}

function windowById(app, id) {
    const windows = app.windows();
    for (let i = 0; i < windows.length; i++) {
        if (String(windows[i].id()) === id) return windows[i];
    }
    fail('That window is no longer open');
}

function tabById(window, id) {
    const tabs = window.tabs();
    for (let i = 0; i < tabs.length; i++) {
        if (String(tabs[i].id()) === id) return tabs[i];
    }
    fail('That tab is no longer open');
}

function perform(argv) {
    const app = Application('/Applications/Helium.app');
    const action = argv[0];
    if (action === 'snapshot') {
        if (!app.running()) return {session: '', tabs: []};
        const generation = session();
        if (!generation) fail('Could not identify the running Helium session');
        const tabs = [];
        const windows = app.windows();
        for (let i = 0; i < windows.length; i++) {
            const window = windows[i];
            const windowId = identifier(window.id());
            const children = window.tabs();
            for (let j = 0; j < children.length; j++) {
                const tab = children[j];
                tabs.push({
                    window_id: windowId,
                    tab_id: identifier(tab.id()),
                    title: tab.title(),
                    url: tab.url()
                });
            }
        }
        if (session() !== generation) fail('Helium restarted; run the command again');
        return {session: generation, tabs: tabs};
    }
    if (action === 'create') {
        if (!app.running()) app.launch();
        let window;
        let tab;
        if (argv[1] === 'window' || app.windows().length === 0) {
            window = app.Window();
            app.windows.push(window);
            tab = window.activeTab();
            tab.url = argv[2];
        } else {
            window = app.windows[0];
            tab = app.Tab({url: argv[2]});
            window.tabs.push(tab);
        }
        app.activate();
        window.minimized = false;
        window.index = 1;
        tab.select();
        const generation = session();
        if (!generation) fail('Could not identify the running Helium session');
        return {
            session: generation,
            window_id: identifier(window.id()),
            tab_id: identifier(tab.id())
        };
    }
    if (action !== 'focus' && action !== 'close-tab' && action !== 'close-window') {
        fail('Unknown automation command');
    }
    if (!app.running() || !argv[1] || session() !== argv[1]) {
        fail('Helium restarted; run the command again');
    }
    const window = windowById(app, argv[2]);
    if (action === 'close-window') {
        window.close();
    } else {
        const tab = tabById(window, argv[3]);
        if (action === 'close-tab') {
            tab.close();
        } else {
            app.activate();
            window.minimized = false;
            window.index = 1;
            tab.select();
        }
    }
    return {};
}

function run(argv) {
    try {
        return JSON.stringify(perform(argv));
    } catch (error) {
        if (String(error).indexOf('-1743') !== -1) throw error;
        return JSON.stringify({error: error.helMessage || 'Helium automation failed; check Automation access and try again'});
    }
}
