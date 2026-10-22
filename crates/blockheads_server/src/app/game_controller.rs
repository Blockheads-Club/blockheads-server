//! `GameController`: owns one hosted world (BHServer + World), the 0.2 s idle timer and the fixed-step loop.
//!
//! `# CLASS GameController` and `## Synthesis resolutions`. Borrow discipline per and
//!  every `Rc` is cloned out of its `RefCell` before calling into it.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::app::integration::Glue;
use crate::app::run_loop::{self, try_catch_perform, RunLoop, TimerId};
use crate::foundation::clock;
use crate::net::bhmatch::{BhMatch, NetNodeType, NodeDelegate, TimerScheduler};
use crate::net::server_match::{BhNetServerMatch, ServerMatchArgs};
use crate::nslog;
use crate::objects::base::{new_mutable_dict, ServerApi};
use crate::plist::value::{PlistDict, PlistValue};
use crate::server::bhserver::{BHServer, ServerDelegate, ServerDynamicWorld, ServerWorld};
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::netdata::ClientMacroBlockRequest;
use crate::types::structs::{IntPair, WindowInfo};
use crate::world::lifecycle::{WorldDelegate, WorldInitArgs};
use crate::world::state::{BHServer as WorldServerHandle, World};

/// The app delegate protocol (implemented by app_cli's `CommandLineDelegate`, held weakly: ObjC `assign`).
pub trait GameControllerDelegate {
    /// `removeGameController:`; the CLI implementation clears its reference and calls `exit(0)`. Never returns there.
    fn remove_game_controller(&self, gc: &Rc<GameController>);
    /// `loadCompleteForGameController:` (empty).
    fn load_complete_for_game_controller(&self, gc: &Rc<GameController>);
    /// `playersChanged` (empty).
    fn players_changed(&self);
    /// `clearChat` (empty).
    fn clear_chat(&self);
    /// `chatMessageRecievedFromGameController:messageDict:` (logs `"%@: %@"` of alias and message).
    fn chat_message_recieved_from_game_controller_message_dict(&self, gc: &Rc<GameController>, d: &PlistDict);
}

/// Arguments of the 16-keyword initializer (`appDatabase*` are always nil and omitted).
pub struct GameControllerArgs {
    pub name: Option<String>,
    pub save_id: Option<String>,
    pub app_delegate: Weak<dyn GameControllerDelegate>,
    pub port: Option<String>,
    pub max_players: i32,
    pub save_delay: i32,
    pub world_width_macro: i32,
    pub credit: i32,
    pub cloud_salt: Option<String>,
    pub owner_name: Option<String>,
    pub privacy: Option<String>,
    pub convert_to_custom_rules: bool,
    pub expert_mode: bool,
    pub no_exit: bool,
}

/// Idle period: `0.2` as f64.
const IDLE_INTERVAL: f64 = 0.2;

/// `dt = (float)(now - lastTime)`, clamped above at 1.0f, never below.
pub fn idle_dt(now: f64, last_time: f64) -> f32 {
    let mut dt = (now - last_time) as f32;
    if dt > 1.0f32 {
        dt = 1.0f32;
    }
    dt
}

/// Loop condition of the fixed-step loop: `(double)accumulator >= 0.2`.
pub fn accumulator_due(acc: f32) -> bool {
    (acc as f64) >= 0.2f64
}

/// `accumulator = (float)((double)accumulator - 0.2)`.
pub fn accumulator_consume(acc: f32) -> f32 {
    ((acc as f64) - 0.2f64) as f32
}

/// Class `GameController`.
pub struct GameController {
    weak_self: Weak<GameController>,
    world: RefCell<Option<Rc<World>>>,
    /// Keeps the `ServerWorld` face of the world alive (BHServer holds it weakly).
    world_face: RefCell<Option<Rc<dyn ServerWorld>>>,
    bh_server: RefCell<Option<Rc<BHServer>>>,
    /// The production seam implementation (app/integration.rs).
    glue: RefCell<Option<Rc<Glue>>>,
    world_name: RefCell<Option<String>>,
    update_timer: Cell<Option<TimerId>>,
    last_time: Cell<f64>,
    accumulator: Cell<f32>,
    needs_to_exit_world: Cell<bool>,
    app_delegate: Weak<dyn GameControllerDelegate>,
    port: RefCell<Option<String>>,
    #[allow(dead_code)]
    save_id: RefCell<Option<String>>,
    chat_messages: RefCell<Vec<PlistDict>>,
    has_had_connected_clients: Cell<bool>,
    no_exit: bool,
}

impl GameController {
    /// The long-selector initializer. `None` == nil (port bind failure, after logging).
    pub fn init_with_world_named_save_id_app_delegate_port_max_players_save_delay_world_width_macro_credit_cloud_salt_owner_name_privacy_convert_to_custom_rules_expert_mode_app_database_app_database_environment_no_exit(
        a: GameControllerArgs,
    ) -> Option<Rc<GameController>> {
        let gc = Rc::new_cyclic(|w| GameController {
            weak_self: w.clone(),
            world: RefCell::new(None),
            world_face: RefCell::new(None),
            bh_server: RefCell::new(None),
            glue: RefCell::new(None),
            world_name: RefCell::new(a.name.clone()),
            update_timer: Cell::new(None),
            last_time: Cell::new(0.0),
            accumulator: Cell::new(0.0),
            needs_to_exit_world: Cell::new(false),
            app_delegate: a.app_delegate.clone(),
            port: RefCell::new(a.port.clone()),
            save_id: RefCell::new(a.save_id.clone()),
            chat_messages: RefCell::new(Vec::new()),
            has_had_connected_clients: Cell::new(false),
            no_exit: a.no_exit,
        });

        // 1. BHNetServerMatch (binds UDP, creates the poll timer first).
        let timers: Rc<dyn TimerScheduler> = RunLoop::current();
        let m = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
            ServerMatchArgs {
                port: a.port.clone(),
                user_name: Some("server".to_string()),
                has_local_player: false,
                world_name: a.name.clone(),
                credit: a.credit as f32,
                cloud_salt: a.cloud_salt.clone(),
                owner_name: a.owner_name.clone(),
                privacy: a.privacy.clone(),
            },
            timers,
        );
        let m = match m {
            Ok(m) => m,
            Err(_) => {
                nslog!(
                    "Failed to start server. Perhaps port {} is already in use?",
                    a.port.as_deref().unwrap_or("(null)")
                );
                return None;
            }
        };

        // 2. BHServer (maxPlayers 0 -> 32).
        let max_players = if a.max_players == 0 { 32 } else { a.max_players };
        let delegate: Weak<dyn ServerDelegate> = Rc::downgrade(&gc) as Weak<GameController>;
        let mm: Rc<dyn BhMatch> = m;
        let server = BHServer::init_with_delegate_match_net_node_type_save_id_max_players(
            delegate,
            mm,
            NetNodeType::OnlineServer,
            a.save_id.as_deref(),
            max_players,
        );
        if let Some(s) = &server {
            s.set_delay_scheduler(Some(Rc::new(|d, cb| {
                run_loop::perform_after_delay(d, cb);
            })));
        }
        *gc.bh_server.borrow_mut() = server.clone();
        // 2b. Install every production seam BEFORE `World::init` (A2): init already calls
        // `tip_manager_set_world_and_reset`, `server_start_bulk_transaction`, `path_creator_init`,...
        let glue = server.as_ref().map(|s| {
            let g = Glue::new(s);
            g.install_pre_init();
            g
        });
        *gc.glue.borrow_mut() = glue.clone();
        // 2c. Mods (not in the original): installed only when mods are present.
        if let Some(s) = &server {
            crate::mods::start(a.save_id.as_deref(), s);
        }

        // 3. windowInfo (PIXEL_SCALE == 1.0f, RESOLVED world_core), serverHostData, custom rules.
        let window_info = WindowInfo {
            screen_width: 1024.0f32,
            screen_height: 768.0f32,
            half_screen_width: 512.0f32,
            half_screen_height: 384.0f32,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
        };
        let mut host = new_mutable_dict();
        if let Some(p) = &a.port {
            host.insert("hostPort".to_string(), PlistValue::string(p.clone()));
            host.insert("maxPlayers".to_string(), PlistValue::string(format!("{}", max_players)));
        }
        let custom_rules = if a.convert_to_custom_rules { Some(new_mutable_dict()) } else { None };

        // 4. World (logs "loading world with size", 500 rand draws).
        let wd: Weak<dyn WorldDelegate> = Rc::downgrade(&gc) as Weak<GameController>;
        // Note: world_state still has a placeholder BHServer type; it carries the `ServerApi` face only.
        let handle = server.as_ref().map(|s| {
            let api: Rc<dyn ServerApi> = s.clone();
            Rc::new(WorldServerHandle { api: Some(api) })
        });
        let world = World::init(WorldInitArgs {
            window_info,
            delegate: wd,
            save_id: a.save_id.clone(),
            name: a.name.clone(),
            server: handle,
            server_host_data: Some(host),
            save_delay: a.save_delay,
            world_width_macro: a.world_width_macro,
            custom_rules,
            expert_mode: a.expert_mode,
        });
        if let (Some(w), Some(g)) = (&world, &glue) {
            g.install_post_init(w);
        }
        if let Some(w) = &world {
            let face: Rc<dyn ServerWorld> = Rc::new(WorldServerAdapter { world: w.clone(), glue: glue.clone() });
            *gc.world_face.borrow_mut() = Some(face);
        }
        *gc.world.borrow_mut() = world;

        // 5. Idle timer: fire date = now, 0.2 s, repeating; closure holds a Weak.
        let weak = gc.weak_self.clone();
        let id = RunLoop::current().add_timer(
            clock::reference_time(),
            IDLE_INTERVAL,
            true,
            Box::new(move || {
                if let Some(g) = weak.upgrade() {
                    g.idle();
                }
            }),
        );
        gc.update_timer.set(Some(id));
        Some(gc)
    }

    /// Alias of the long initializer.
    pub fn init(a: GameControllerArgs) -> Option<Rc<GameController>> {
        Self::init_with_world_named_save_id_app_delegate_port_max_players_save_delay_world_width_macro_credit_cloud_salt_owner_name_privacy_convert_to_custom_rules_expert_mode_app_database_app_database_environment_no_exit(a)
    }

    fn rc(&self) -> Rc<GameController> {
        self.weak_self.upgrade().expect("GameController alive")
    }

    /// `-idle`: timer callback inside the TryCatch boundary.
    pub fn idle(self: &Rc<Self>) {
        let me = self.clone();
        try_catch_perform("doIdleSafe", move || me.do_idle_safe());
    }

    /// `-doIdleSafe`.
    pub fn do_idle_safe(self: &Rc<Self>) {
        self.do_idle_safe_with_now(clock::reference_time());
    }

    /// `-doIdleSafe` with an explicit clock reading (test hook).
    pub fn do_idle_safe_with_now(self: &Rc<Self>, now: f64) {
        let dt = idle_dt(now, self.last_time.get());
        if let Some(w) = self.world() {
            w.pre_update(dt);
        }
        if !self.is_running() {
            if !self.no_exit {
                let w = self.world();
                if let Some(w) = &w {
                    if !self.has_had_connected_clients.get() || !w.has_finished_database_migration_to17() {
                        self.last_time.set(now);
                        return;
                    }
                }
                self.stop();
                if let Some(d) = self.app_delegate.upgrade() {
                    d.remove_game_controller(self);
                }
            }
        } else {
            self.accumulator.set(self.accumulator.get() + dt);
            while accumulator_due(self.accumulator.get()) {
                self.update_accurate_dt(0.2f32, 0.2f32);
                self.accumulator.set(accumulator_consume(self.accumulator.get()));
            }
        }
        self.last_time.set(now);
    }

    /// `-update:accurateDT:`.
    pub fn update_accurate_dt(&self, dtf: f32, accurate_dt: f32) {
        let Some(world) = self.world() else { return };
        if !world.load_complete() {
            world.incremental_load();
            if world.load_complete() {
                world.save_all();
                world.finish_bulk_database_update();
                let server = self.bh_server();
                let face = self.world_face.borrow().clone();
                if let (Some(s), Some(f)) = (server, face) {
                    s.set_world(&f);
                }
                if let Some(d) = self.app_delegate.upgrade() {
                    d.load_complete_for_game_controller(&self.rc());
                }
                nslog!("World load complete.");
                if let Some(h) = crate::mods::hooks() {
                    h.world_loaded(&world.world_name().unwrap_or_default(), &world.save_id().unwrap_or_default());
                }
            }
        } else {
            world.update_accurate_dt_pinch_scale_drag_in_progress(dtf, accurate_dt, 1.0, false);
            if let Some(h) = crate::mods::hooks() {
                h.tick(dtf);
            }
            if self.needs_to_exit_world.get() {
                // doExitWorld: empty stub on the server.
            }
        }
    }

    /// `-cleanupWorld`.
    pub fn cleanup_world(&self) {
        if let Some(w) = self.world() {
            w.delete_timers();
        }
        let s = self.bh_server();
        if let Some(s) = s {
            s.cleanup(); // re-enters players_changed; no borrow of our fields is held
        }
        *self.bh_server.borrow_mut() = None;
        let w = self.world.borrow_mut().take();
        let face = self.world_face.borrow_mut().take();
        if let Some(w) = w {
            w.dealloc(); // logs "Exiting World."
        }
        drop(face);
        if let Some(g) = self.glue.borrow_mut().take() {
            g.shutdown();
        }
    }

    /// `-stop`.
    pub fn stop(&self) {
        if let Some(w) = self.world() {
            w.save_all();
            w.finish_bulk_database_update();
        }
        crate::mods::stop();
        // setPaused:NO is a no-op.
        self.cleanup_world();
    }

    /// `-isRunning`.
    pub fn is_running(&self) -> bool {
        let Some(world) = self.world() else { return false };
        let mut running = !self.connected_client_ids().is_empty();
        if !running {
            running = self.loading();
        }
        if !running {
            let n = world.current_total_blockhead_count_including_net();
            running = n >= 1;
            if n < 1 {
                running = world.server_clients_count() != 0;
            }
        }
        running
    }

    /// `-loading`: `![world loadComplete]` (YES for a nil world).
    pub fn loading(&self) -> bool {
        match self.world() {
            Some(w) => !w.load_complete(),
            None => true,
        }
    }

    /// `-recentPlayers`.
    pub fn recent_players(&self) -> Vec<PlistDict> {
        self.bh_server().map(|s| s.recent_players()).unwrap_or_default()
    }

    /// `-connectedClientIDs`.
    pub fn connected_client_ids(&self) -> Vec<String> {
        self.bh_server().map(|s| s.connected_client_ids()).unwrap_or_default()
    }

    /// `-port`.
    pub fn port(&self) -> Option<String> {
        self.port.borrow().clone()
    }

    /// `-worldName`.
    pub fn world_name(&self) -> Option<String> {
        self.world_name.borrow().clone()
    }

    /// `-playersChanged`: short-circuit order as written.
    pub fn players_changed(&self) {
        if let Some(d) = self.app_delegate.upgrade() {
            d.players_changed();
        }
        let v = self.has_had_connected_clients.get()
            || !self.connected_client_ids().is_empty()
            || self.world().map_or(false, |w| w.current_total_blockhead_count_including_net() > 0)
            || self.world().map_or(false, |w| w.server_clients_count() != 0);
        self.has_had_connected_clients.set(v);
    }

    /// `-chatMessageRecieved:displayNotification:` (flag ignored).
    pub fn chat_message_recieved_display_notification(&self, d: PlistDict, _display_notification: bool) {
        {
            let mut c = self.chat_messages.borrow_mut();
            c.insert(0, d.clone());
            if c.len() > 1000 {
                c.pop();
            }
        }
        if let Some(del) = self.app_delegate.upgrade() {
            del.chat_message_recieved_from_game_controller_message_dict(&self.rc(), &d);
        }
    }

    /// `-clearChat`.
    pub fn clear_chat(&self) {
        self.chat_messages.borrow_mut().clear();
        if let Some(del) = self.app_delegate.upgrade() {
            del.clear_chat();
        }
    }

    /// `-chatMessages` (newest first).
    pub fn chat_messages(&self) -> Vec<PlistDict> {
        self.chat_messages.borrow().clone()
    }

    /// `-sendChatMessage:sendToClients:`.
    pub fn send_chat_message_send_to_clients(&self, text: Option<&str>, clients: Option<&[String]>) {
        if let Some(s) = self.bh_server() {
            s.send_chat_message_send_to_clients(text, clients);
        }
    }

    /// `-fileWriteFailed:`.
    pub fn file_write_failed(&self, save_path: &str) {
        nslog!("failed to write to file:{}", save_path);
    }

    /// `-full`.
    pub fn full(&self) -> bool {
        self.bh_server().map_or(false, |s| s.full())
    }

    /// `-isControllingBlockheadsForClientPlayer:`.
    pub fn is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
        self.world().map_or(false, |w| w.is_controlling_blockheads_for_client_player(player_id))
    }

    /// `-handleCommand:issueClient:`; the BHServer result is dropped. The server is cloned out first because
    /// `/stop` drops the controller's reference while the method runs.
    pub fn handle_command_issue_client(&self, command: &str, issue_client: Option<&str>) {
        if let Some(s) = self.bh_server() {
            s.handle_command_issue_client(command, issue_client);
        }
    }

    /// `-isWaitingForMultiPartCommandResponse`.
    pub fn is_waiting_for_multi_part_command_response(&self) -> bool {
        self.bh_server().map_or(false, |s| s.is_waiting_for_multi_part_command_response())
    }

    /// `-whiteList` (forwarder).
    pub fn white_list(&self) -> Option<Vec<String>> {
        self.bh_server().and_then(|s| s.white_list())
    }
    /// `-blackList`.
    pub fn black_list(&self) -> Option<Vec<String>> {
        self.bh_server().and_then(|s| s.black_list())
    }
    /// `-adminList`.
    pub fn admin_list(&self) -> Option<Vec<String>> {
        self.bh_server().and_then(|s| s.admin_list())
    }
    /// `-modList`.
    pub fn mod_list(&self) -> Option<Vec<String>> {
        self.bh_server().and_then(|s| s.mod_list())
    }

    /// `-playerIsBlackListedWithInfo:`.
    pub fn player_is_black_listed_with_info(&self, info: &PlistDict) -> bool {
        self.bh_server().map_or(false, |s| s.player_is_black_listed_with_info(info))
    }
    /// `-playerIsWhiteListedWithInfo:`.
    pub fn player_is_white_listed_with_info(&self, info: &PlistDict) -> bool {
        self.bh_server().map_or(false, |s| s.player_is_white_listed_with_info(info))
    }
    /// `-playerIsConnectedWithInfo:`.
    pub fn player_is_connected_with_info(&self, info: &PlistDict) -> bool {
        self.bh_server().map_or(false, |s| s.player_is_connected_with_info(info))
    }
    /// `-playerIsAdminWithInfo:` -> `playerIsAdminWithAlias:info[@"alias"]`.
    pub fn player_is_admin_with_info(&self, info: &PlistDict) -> bool {
        let alias = info.get(&"alias".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        self.bh_server().map_or(false, |s| s.player_is_admin_with_alias(alias.as_deref()))
    }
    /// `-playerIsModWithInfo:`.
    pub fn player_is_mod_with_info(&self, info: &PlistDict) -> bool {
        let alias = info.get(&"alias".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        self.bh_server().map_or(false, |s| s.player_is_mod_with_alias(alias.as_deref()))
    }

    /// `-world` (cloned out of the cell).
    pub fn world(&self) -> Option<Rc<World>> {
        self.world.borrow().clone()
    }

    /// `-bhServer` (cloned out of the cell).
    pub fn bh_server(&self) -> Option<Rc<BHServer>> {
        self.bh_server.borrow().clone()
    }

    /// `-dealloc`: invalidates the update timer. Called by the owner when it drops the controller.
    pub fn dealloc(&self) {
        if let Some(id) = self.update_timer.take() {
            run_loop::invalidate(id);
        }
    }
}

/// `WorldDelegate` (state placeholder): `appDatabase` is always nil.
impl crate::world::state::WorldDelegate for GameController {
    fn app_database(&self) -> Option<Rc<Database>> {
        None
    }
}

/// `WorldDelegate`.
impl WorldDelegate for GameController {
    fn get_default_game_save_for_world_with_id(&self, _save_id: &str) -> PlistDict {
        new_mutable_dict()
    }
    fn file_write_failed(&self, save_path: &str) {
        GameController::file_write_failed(self, save_path)
    }
    fn app_database_environment(&self) -> Option<Rc<DatabaseEnvironment>> {
        None
    }
    fn show_welcome_back_popup_with_message(&self, _message: Option<&str>) {}
    fn set_paused(&self, _paused: bool) {}
}

/// `BHNetNodeDelegate`.
impl NodeDelegate for GameController {
    fn players_changed(&self) {
        GameController::players_changed(self)
    }
}

/// `BHServer.delegate`.
impl ServerDelegate for GameController {
    fn chat_message_recieved_display_notification(&self, msg: PlistDict, display_notification: bool) {
        GameController::chat_message_recieved_display_notification(self, msg, display_notification)
    }
    fn clear_chat(&self) {
        GameController::clear_chat(self)
    }
    fn stop(&self) {
        GameController::stop(self)
    }
}

/// `ServerWorld` face of `World` handed to `BHServer.setWorld:`; pure forwarding.
/// `server_client` / `dynamic_world` return the real objects held by the integration glue.
pub struct WorldServerAdapter {
    pub(crate) world: Rc<World>,
    pub(crate) glue: Option<Rc<Glue>>,
}

impl ServerWorld for WorldServerAdapter {
    fn load_complete(&self) -> bool {
        self.world.load_complete()
    }
    fn world_name(&self) -> Option<String> {
        self.world.world_name()
    }
    fn save_id(&self) -> Option<String> {
        self.world.save_id()
    }
    fn world_width_macro(&self) -> i32 {
        self.world.world_width_macro()
    }
    fn custom_rules_enabled(&self) -> bool {
        self.world.custom_rules().custom_rules_enabled != 0
    }
    fn custom_rules_dict(&self) -> Option<PlistValue> {
        self.world.custom_rules_dict().map(PlistValue::Dict)
    }
    fn server_password(&self) -> Option<String> {
        self.world.server_password()
    }
    fn welcome_message(&self) -> Option<String> {
        self.world.welcome_message()
    }
    fn pvp_enabled(&self) -> bool {
        self.world.pvp_enabled()
    }
    fn set_pvp_enabled_display_notifciation(&self, enabled: bool, display: bool) {
        self.world.set_pvp_enabled_display_notifciation(enabled, display)
    }
    fn new_server_password_set(&self, password: Option<&str>) {
        self.world.new_server_password_set(password)
    }
    fn server_client_count(&self) -> usize {
        self.world.server_clients_count()
    }
    fn server_client(&self, id: &str) -> Option<Rc<crate::server::server_client::ServerClient>> {
        // Same map key set as `World.serverClients` (nil map / missing key -> None). A1.
        if !self.world.has_server_client(id) {
            return None;
        }
        self.glue.as_ref().and_then(|g| g.client(id))
    }
    fn dynamic_world(&self) -> Option<Rc<dyn ServerDynamicWorld>> {
        // `None` only while the World has no DynamicWorld (before load stage 12). A1.
        if self.world.dynamic_world().is_none() {
            return None;
        }
        self.glue.as_ref().and_then(|g| g.dw_face())
    }
    fn distance_ordered_food_types(&self) -> Vec<i32> {
        self.world.distance_ordered_food_types.borrow().iter().map(|v| *v as i32).collect()
    }
    fn has_finished_database_migration_to17(&self) -> bool {
        self.world.has_finished_database_migration_to17()
    }
    fn append_debug_log(&self, out: &mut String) {
        self.world.append_debug_log(out)
    }
    fn summary_net_data_for_admin_mod_owner_cloud_mode(
        &self,
        admin: bool,
        moderator: bool,
        owner: bool,
        cloud_mode: i32,
    ) -> Vec<u8> {
        self.world.summary_net_data_for_admin_mod_owner_cloud_mode(admin, moderator, owner, cloud_mode)
    }
    fn client_connected(&self, id: &str) -> bool {
        self.world.client_connected(id)
    }
    fn client_disconnected_was_kick(&self, id: &str, was_kick: bool) {
        self.world.client_disconnected_was_kick(id, was_kick)
    }
    fn request_for_block_from_client(&self, req: ClientMacroBlockRequest, id: &str) {
        self.world.request_for_block_from_client(req, id)
    }
    fn remote_block_removed_by_client(&self, macro_index: i32, id: &str) {
        self.world.remote_block_removed_by_client(macro_index, id)
    }
    fn remote_create_for_objects_of_type_client_id(&self, items: Vec<Vec<u8>>, ty: i32, id: &str) {
        self.world.remote_create_for_objects_of_type_client_id(items, ty, id)
    }
    fn remote_update_for_objects_of_type_from_client(&self, items: Vec<Vec<u8>>, ty: i32, id: &str) {
        self.world.remote_update_for_objects_of_type_from_client(items, ty, Some(id))
    }
    fn remote_remove_for_objects_of_type_from_client(&self, items: Vec<Vec<u8>>, ty: i32, id: &str) {
        self.world.remote_remove_for_objects_of_type_from_client(items, ty, Some(id))
    }
    fn remote_creation_data_update_for_objects_of_type_from_client(&self, items: Vec<Vec<u8>>, ty: i32, id: &str) {
        self.world.remote_creation_data_update_for_objects_of_type_from_client(items, ty, Some(id))
    }
    fn remote_fill_request_placed_by_client(&self, payload: &[u8], id: &str) -> u32 {
        self.world.remote_fill_request_placed_by_client(payload, Some(id))
    }
    fn remote_remove_request_from_client(&self, payload: &[u8], id: &str) -> u32 {
        self.world.remote_remove_request_from_client(payload, Some(id))
    }
    fn remote_remove_back_wall_request_from_client(&self, payload: &[u8], id: &str) -> u32 {
        self.world.remote_remove_back_wall_request_from_client(payload, Some(id))
    }
    fn remote_place_workbench_request_from_peer(&self, payload: &[u8], id: &str) {
        self.world.remote_place_workbench_request_from_peer(payload, Some(id))
    }
    fn remote_place_interaction_object_request_from_peer(&self, payload: &[u8], id: &str) {
        self.world.remote_place_interaction_object_request_from_peer(payload, Some(id))
    }
    fn remote_paint_request_from_client(&self, payload: &[u8], id: &str) {
        self.world.remote_paint_request_from_client(payload, Some(id))
    }
    fn heartbeat_data_recieved_from_peer(&self, payload: &[u8], id: &str) {
        self.world.heartbeat_data_recieved_from_peer(payload, id)
    }
    fn remote_gather_request(&self, payload: &[u8]) {
        self.world.remote_gather_request(payload)
    }
    fn remote_projectile_request(&self, payload: &[u8]) {
        self.world.remote_projectile_request(payload)
    }
    fn remote_tip_notification(&self, payload: &[u8]) {
        self.world.remote_tip_notification(payload)
    }
    fn remote_blockhead_damage_request_requested_by_client_name(&self, payload: &[u8], name: Option<&str>) {
        self.world.remote_blockhead_damage_request_requested_by_client_name(payload, name)
    }
    fn electricity_path_data_recieved_from_client(&self, payload: &[u8], id: &str) {
        self.world.electricity_path_data_recieved_from_client(payload, id)
    }
    fn get_blockhead_zoom_pos_for_client_requester_client_cycle_index(
        &self,
        player: Option<&str>,
        requester: Option<&str>,
        cycle_index: i32,
    ) -> IntPair {
        self.world.get_blockhead_zoom_pos_for_client_requester_client_cycle_index(player, requester, cycle_index)
    }
    fn transaction_data_recieved_from_client(&self, dict: Option<&PlistValue>, id: &str) {
        self.world.transaction_data_recieved_from_client(dict, id)
    }
    fn welcome_message_dict_recieved_from_client(&self, dict: Option<&PlistValue>, id: &str) {
        self.world.welcome_message_dict_recieved_from_client(dict, id)
    }
    fn report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
        &self,
        name: Option<&str>,
        reporter_name: Option<&str>,
        reporter_message: Option<&str>,
        avatar_path: Option<&str>,
    ) {
        self.world.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
            name,
            reporter_name,
            reporter_message,
            avatar_path,
        )
    }
    fn verify_client_custom_rules_data(&self, payload: &[u8]) -> bool {
        self.world.verify_client_custom_rules_data(payload)
    }
    fn custom_rule_dict_recieved_from_net(&self, dict: &PlistValue) {
        if let PlistValue::Dict(d) = dict {
            self.world.custom_rule_dict_recieved_from_net(d)
        }
    }
    fn archive_light_blocks_for_client(&self, id: &str) {
        self.world.archive_light_blocks_for_client(id)
    }
    fn do_repair_for_tile_at_pos(&self, pos: IntPair) {
        self.world.do_repair_for_tile_at_pos(pos)
    }
    fn world_pos_at_world_index(&self, index: i32) -> IntPair {
        crate::world::tiles::world_pos_at_world_index(index, &self.world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T1: accumulator walk: first idle (dt clamp 1.0) gives 5 updates, then exactly 1 per 0.2f tick.
    #[test]
    fn t1_accumulator() {
        let mut acc = 0.0f32;
        let mut counts = vec![];
        for dt in std::iter::once(1.0f32).chain(std::iter::repeat(0.2f32).take(10)) {
            acc += dt;
            let mut n = 0;
            while accumulator_due(acc) {
                n += 1;
                acc = accumulator_consume(acc);
            }
            counts.push(n);
            assert_eq!(acc as f64, 3.278255533700758e-08);
        }
        assert_eq!(counts, vec![5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
    }

    /// T2: dt clamp above only.
    #[test]
    fn t2_dt_clamp() {
        assert_eq!(idle_dt(5.0, 0.0), 1.0);
        assert_eq!(idle_dt(10.0, 15.0), -5.0);
        assert_eq!(idle_dt(1.0, 1.3), (-0.3f64) as f32);
        let mut acc = 0.0f32;
        acc += idle_dt(1.0, 1.3);
        assert!(!accumulator_due(acc) && acc < 0.0);
    }
}
