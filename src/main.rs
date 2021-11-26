use cpp_core::{CppBox, Ptr, StaticUpcast};
use dbus::{blocking::Connection, Message};
use gamemode::{
    ComFeralinteractiveGameModeGameRegistered, ComFeralinteractiveGameModeGameUnregistered,
};
use qt_core::{slot, QBox, QObject, QString, QTimer, SlotNoArgs};
use qt_gui::QIcon;
use qt_widgets::{QApplication, QSystemTrayIcon};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::{
        mpsc::{channel, Receiver},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use sysinfo::{ProcessExt, System, SystemExt};

mod gamemode;

struct GameModeTray {
    system_tray: QBox<QSystemTrayIcon>,
    off_icon: CppBox<QString>,
    on_icon: CppBox<QString>,
    game_mode_rx: Receiver<Vec<String>>,
    timer: QBox<QTimer>,
}

impl StaticUpcast<QObject> for GameModeTray {
    unsafe fn static_upcast(ptr: Ptr<Self>) -> Ptr<QObject> {
        ptr.system_tray.as_ptr().static_upcast()
    }
}

impl GameModeTray {
    fn new(game_mode_rx: Receiver<Vec<String>>) -> Rc<GameModeTray> {
        unsafe {
            let system_tray = QSystemTrayIcon::new();
            let off_icon = QString::from_std_str("icons/game_mode_off.ico");
            let on_icon = QString::from_std_str("icons/game_mode_on.ico");

            let timer = QTimer::new_0a();
            timer.start_1a(1000);

            let this = Rc::new(Self {
                system_tray,
                off_icon,
                on_icon,
                game_mode_rx,
                timer,
            });

            this.init();
            this
        }
    }

    fn init(self: &Rc<Self>) {
        self.disable_game_mode();

        unsafe {
            let signal = self.timer.timeout();

            signal.connect(&self.slot_check_game_mode_state());
        }
    }

    #[slot(SlotNoArgs)]
    unsafe fn check_game_mode_state(self: &Rc<Self>) {
        if let Ok(games) = self.game_mode_rx.try_recv() {
            if games.len() > 0 {
                self.enable_game_mode(games);
            } else {
                self.disable_game_mode();
            }
        }
    }

    fn enable_game_mode(self: &Rc<Self>, games: Vec<String>) {
        unsafe {
            let icon = QIcon::new();

            icon.add_file_1a(&self.on_icon);

            self.system_tray.set_icon(&icon);

            self.system_tray
                .set_tool_tip(&QString::from_std_str(format!(
                    "Game Mode On:\n{}",
                    games.join("\n")
                )));
        }
    }

    fn disable_game_mode(self: &Rc<Self>) {
        unsafe {
            let icon = QIcon::new();

            icon.add_file_1a(&self.off_icon);

            self.system_tray.set_icon(&icon);

            self.system_tray
                .set_tool_tip(&QString::from_std_str("Game Mode Off"));
        }
    }

    fn show(self: &Rc<Self>) {
        unsafe {
            self.system_tray.set_visible(true);
        }
    }
}

fn start_game_mode_monitor() -> Receiver<Vec<String>> {
    let (tx, rx) = channel::<Vec<String>>();

    let conn = Connection::new_session().unwrap();

    let proxy = conn.with_proxy(
        "com.feralinteractive.GameMode",
        "/com/feralinteractive/GameMode",
        Duration::from_millis(5000),
    );

    let processes = Arc::new(Mutex::new(HashMap::new()));

    {
        let processes = processes.clone();
        let tx = tx.clone();

        let _id = proxy.match_signal(
            move |h: ComFeralinteractiveGameModeGameRegistered, _: &Connection, _: &Message| {
                let pid = h.arg0;
                let mut system = System::new();
                system.refresh_processes();
                let process_name = if let Some(p) = system.process(pid) {
                    p.name().to_string()
                } else {
                    "Unknown".to_string()
                };

                let mut processes = processes.lock().unwrap();
                processes.insert(h.arg0, process_name);

                tx.send(processes.values().cloned().collect()).unwrap();
                true
            },
        );
    }

    {
        let _id = proxy.match_signal(
            move |h: ComFeralinteractiveGameModeGameUnregistered, _: &Connection, _: &Message| {
                let mut processes = processes.lock().unwrap();
                processes.remove(&h.arg0);
                tx.send(processes.values().cloned().collect()).unwrap();
                true
            },
        );
    }

    thread::spawn(move || loop {
        conn.process(Duration::from_millis(1000)).unwrap();
    });

    rx
}

fn main() {
    QApplication::init(|_| {
        let game_mode_rx = start_game_mode_monitor();
        let game_mode_tray = GameModeTray::new(game_mode_rx);

        game_mode_tray.show();

        unsafe { QApplication::exec() }
    })
}
