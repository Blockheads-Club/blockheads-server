//! `TipManager`: the HUD tip state machine. The text never leaves the process on the server, but its state decides
//! when `Blockhead.tipType` is cleared (gate at line 454), so it is ported exactly.
//!
//!  TipManager" (5.1-5.3.2);

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::foundation::clock::reference_time;
use crate::objects::handle::ObjRef;
use crate::world::state::World;

/// `TipType` (Blockhead.tipType).
pub mod tip_type {
    /// `NONE`
    pub const NONE: i32 = 0;
    /// `COLLAPSE_EXHAUSTED`
    pub const COLLAPSE_EXHAUSTED: i32 = 1;
    /// `COLLAPSE_STARVATION`
    pub const COLLAPSE_STARVATION: i32 = 2;
    /// `REFUSES_TO_WORK_EXHAUSTED`
    pub const REFUSES_TO_WORK_EXHAUSTED: i32 = 3;
    /// `REFUSES_TO_WORK_STARVATION`
    pub const REFUSES_TO_WORK_STARVATION: i32 = 4;
    /// `NO_FUEL`
    pub const NO_FUEL: i32 = 5;
    /// `NO_ELECTRICITY`
    pub const NO_ELECTRICITY: i32 = 6;
    /// `SUFFER_DAMAGE`
    pub const SUFFER_DAMAGE: i32 = 7;
    /// `REGENERATING`
    pub const REGENERATING: i32 = 8;
    /// `DYING`
    pub const DYING: i32 = 9;
}

/// The `Blockhead` getters TipManager uses. Implemented by the blockhead
/// module's adapter returned from [`TipBlockheadProvider`]. All are side-effect free except `set_tip_type`.
pub trait TipBlockhead {
    /// `-name`
    fn name(&self) -> String;
    /// `-tipType`
    fn tip_type(&self) -> i32;
    /// `-setTipType:`
    fn set_tip_type(&self, t: i32);
    /// `-health`
    fn health(&self) -> f32;
    /// `-happiness`
    fn happiness(&self) -> f32;
    /// `-fullness`
    fn fullness(&self) -> f32;
    /// `-energy`
    fn energy(&self) -> f32;
    /// `-environment`
    fn environment(&self) -> f32;
    /// `-environmentLight`
    fn environment_light(&self) -> f32;
    /// `-environmentExposure`
    fn environment_exposure(&self) -> f32;
    /// `-environmentTemperature`
    fn environment_temperature(&self) -> f32;
    /// `-currentTemperature`
    fn current_temperature(&self) -> f32;
    /// `-isMale`
    fn is_male(&self) -> bool;
    /// `-regenerating`
    fn regenerating(&self) -> bool;
    /// `-onTradeMission`
    fn on_trade_mission(&self) -> bool;
    /// `-asleep`
    fn asleep(&self) -> bool;
    /// `-meditating`
    fn meditating(&self) -> bool;
    /// `-currentInteractionType`
    fn current_interaction_type(&self) -> i32;
    /// `-currentInteractionIsGoodOrBad` (no side effects,)
    fn current_interaction_is_good_or_bad(&self) -> i32;
}

/// Adapter factory installed by the integrator: turns a Blockhead `ObjRef` into a [`TipBlockhead`] view.
pub trait TipBlockheadProvider {
    /// `None` when `b` is not a Blockhead.
    fn tip_blockhead(&self, b: &ObjRef) -> Option<Rc<dyn TipBlockhead>>;
}

thread_local! {
    static PROVIDER: RefCell<Option<Rc<dyn TipBlockheadProvider>>> = const { RefCell::new(None) };
    static INSTANCE: RefCell<Option<Rc<TipManager>>> = const { RefCell::new(None) };
}

/// Install the Blockhead view provider (main thread).
pub fn install_tip_blockhead_provider(p: Rc<dyn TipBlockheadProvider>) {
    PROVIDER.with(|c| *c.borrow_mut() = Some(p));
}

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DAMAGE_RED: [f32; 4] = [1.0, 0.5, 0.5, 1.0];

/// TipManager ivars (instance size 0x40).
pub struct TipManagerState {
    /// `world` (assign)
    pub world: Weak<World>,
    /// `currentTipText`
    pub current_tip_text: Option<String>,
    /// `timeoutTimer`
    pub timeout_timer: f32,
    /// `timeLastSecondBlockheadTipDisplayed`
    pub time_last_second_blockhead_tip_displayed: f64,
    /// `tipColor`
    pub tip_color: [f32; 4],
    /// `tutorialTipText`
    pub tutorial_tip_text: Option<String>,
}

/// `TipManager: NSObject` singleton.
pub struct TipManager {
    st: RefCell<TipManagerState>,
}

impl TipManager {
    /// `+[TipManager instance]`
    pub fn instance() -> Rc<TipManager> {
        INSTANCE.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(Rc::new(TipManager::new_uninstanced()));
            }
            c.as_ref().unwrap().clone()
        })
    }

    /// A fresh manager (`.cxx_construct`: default `Vector` = (1,1,1,1) tip colour). Not the singleton (tests).
    pub fn new_uninstanced() -> TipManager {
        TipManager {
            st: RefCell::new(TipManagerState {
                world: Weak::new(),
                current_tip_text: None,
                timeout_timer: 0.0,
                time_last_second_blockhead_tip_displayed: 0.0,
                tip_color: [1.0; 4],
                tutorial_tip_text: None,
            }),
        }
    }

    /// `-setWorld:`
    pub fn set_world(&self, w: &Rc<World>) {
        self.st.borrow_mut().world = Rc::downgrade(w);
    }

    /// `-reset`
    pub fn reset(&self) {
        let mut st = self.st.borrow_mut();
        st.current_tip_text = None;
        st.tutorial_tip_text = None;
        st.timeout_timer = 0.0;
        st.time_last_second_blockhead_tip_displayed = 0.0;
    }

    /// `-displayTip:withTimeOut:displayEvenIfDisabled:tipColor:`
    pub fn display_tip_with_time_out_display_even_if_disabled_tip_color(
        &self,
        text: &str,
        timeout: f32,
        force: bool,
        color: [f32; 4],
    ) {
        // [[NSUserDefaults standardUserDefaults] boolForKey:@"dontDisplayTipsv2"]: always NO on the server (R7)
        let dont_display_tips = false;
        if !force && dont_display_tips {
            return;
        }
        let mut st = self.st.borrow_mut();
        if st.current_tip_text.is_none() {
            st.current_tip_text = Some(text.to_string());
            st.timeout_timer = timeout;
        } else if st.current_tip_text.as_deref() == Some(text) {
            st.timeout_timer = timeout; // refresh only; text unchanged
        }
        st.tip_color = color; // always overwritten
    }

    /// `-update:`
    pub fn update(&self, dt: f32) {
        let mut st = self.st.borrow_mut();
        if st.timeout_timer > 0.0 {
            st.timeout_timer -= dt;
            if st.timeout_timer <= 0.0 {
                st.current_tip_text = None;
            }
        }
    }

    /// `-currentTip`
    pub fn current_tip(&self) -> Option<String> {
        let st = self.st.borrow();
        st.current_tip_text.clone().or_else(|| st.tutorial_tip_text.clone())
    }

    /// `-tipColor`
    pub fn tip_color(&self) -> [f32; 4] {
        let st = self.st.borrow();
        if st.current_tip_text.is_none() {
            WHITE
        } else {
            st.tip_color
        }
    }

    /// `-setTutorialTipText:`
    pub fn set_tutorial_tip_text(&self, t: Option<&str>) {
        self.st.borrow_mut().tutorial_tip_text = t.map(|s| s.to_string());
    }

    /// `-updateTipForBlockhead:severityLevel:` through the installed [`TipBlockheadProvider`].
    pub fn update_tip_for_blockhead_severity_level(&self, b: Option<&ObjRef>, level: i32) {
        let Some(b) = b else { return };
        let provider = PROVIDER.with(|c| c.borrow().clone());
        let view = provider.and_then(|p| p.tip_blockhead(b));
        if let Some(v) = view {
            self.update_tip_for_blockhead_view(Some(&*v), level);
        }
    }

    /// Core of `-updateTipForBlockhead:severityLevel:`: does nothing if `b == nil` or a tip is showing.
    pub fn update_tip_for_blockhead_view(&self, b: Option<&dyn TipBlockhead>, level: i32) {
        let Some(b) = b else { return };
        if self.st.borrow().current_tip_text.is_some() {
            return;
        }
        let name = b.name();
        let tip = |fmt: &str| fmt.replacen("%@", &name, 1);
        let show = |text: String, timeout: f32, force: bool, color: [f32; 4]| {
            self.display_tip_with_time_out_display_even_if_disabled_tip_color(&text, timeout, force, color);
        };
        if level == 0 {
            match b.tip_type() {
                tip_type::SUFFER_DAMAGE => {
                    b.set_tip_type(0);
                    show(tip("%@ IS BEING HARMED!"), 10.0, true, DAMAGE_RED);
                }
                tip_type::DYING => {
                    b.set_tip_type(0);
                    show(tip("%@ IS ABOUT TO DIE!"), 10.0, true, DAMAGE_RED);
                }
                tip_type::REGENERATING => {
                    b.set_tip_type(0);
                    show(tip("%@ HAS DIED\nAND IS NOW REGENERATING!"), 10.0, true, DAMAGE_RED);
                }
                _ => {}
            }
            return;
        }
        if level == 1 {
            let t = b.tip_type();
            if t != tip_type::NONE {
                b.set_tip_type(0);
                let text = match t {
                    1 => Some("%@ COLLAPSED\nFROM EXHAUSTION."),
                    2 => Some("%@ COLLAPSED\nFROM STARVATION."),
                    3 => Some("%@ REFUSES TO WORK\nDUE TO EXHAUSTION."),
                    4 => Some("%@ REFUSES TO WORK\nDUE TO STARVATION."),
                    5 => Some("%@ IS OUT OF FUEL."),
                    6 => Some("%@ NEEDS ELECTRICITY."),
                    _ => None, // 7, 8, 9 (and unknown) only clear the tip type
                };
                if let Some(f) = text {
                    show(tip(f), 10.0, true, WHITE);
                }
            }
            return;
        }
        // level 2 and any level other than 0/1
        if level == 2 && !b.regenerating() && !b.on_trade_mission() {
            let health = b.health();
            let happiness = b.happiness();
            if (happiness as f64) >= 0.05f64 && health >= 0.1f32 {
                return;
            }
            let mut is_injured = false;
            let mut new_tip: Option<String> = None;
            let fullness = b.fullness();
            let energy = b.energy();
            let environment = b.environment();
            let pron = if b.is_male() { "HE" } else { "SHE" };
            let ptip = |fmt: &str| fmt.replacen("%@", pron, 1);
            if health < 0.1f32 {
                is_injured = true;
                new_tip = Some("HEAL BY EATING FOOD.".to_string());
            } else if fullness < 0.1f32 {
                new_tip = Some(ptip("BECAUSE %@ IS STARVING."));
            } else if energy < 0.1f32 {
                new_tip = Some(ptip("BECAUSE %@ IS EXHAUSTED."));
            } else if environment < 0.1f32 {
                let light = b.environment_light();
                let exposure = b.environment_exposure();
                let env_temp = b.environment_temperature();
                if env_temp < 0.1f32 {
                    new_tip = Some(if b.current_temperature() <= 15.0f32 {
                        ptip("BECAUSE %@ IS FREEZING.")
                    } else {
                        ptip("BECAUSE %@ IS TOO HOT.")
                    });
                } else if light < 0.1f32 {
                    new_tip = Some(ptip("BECAUSE %@ CAN'T SEE."));
                } else if exposure < 0.1f32 {
                    new_tip = Some(ptip("BECAUSE %@ IS EXPOSED."));
                }
            }
            let Some(new_tip) = new_tip else { return };
            let mut action: Option<String> = None;
            if is_injured {
                action = Some(tip("%@ IS BADLY INJURED,"));
            } else if !b.asleep() && !b.meditating() {
                action = Some(tip("%@ IS MOVING SLOWLY"));
                let it = b.current_interaction_type();
                if (1..=5).contains(&it) || it == 23 || it == 24 {
                    action = Some(tip("%@ IS WORKING SLOWLY"));
                }
            }
            let Some(action) = action else { return };
            show(format!("{}\n{}", action, new_tip), 1.0, false, WHITE);
            return;
        }
        // reached when level != 2 (not 0/1) or when regenerating / onTradeMission
        if b.happiness() > 0.05f32 && b.current_interaction_is_good_or_bad() == -1 {
            show("TIP: YOU GET MORE, FASTER IF YOU\nUSE THE CORRECT TOOL FOR THE JOB.".to_string(), 1.0, false, WHITE);
        }
    }

    /// `-displaySecondBlockheadTipIfGoodTime`
    pub fn display_second_blockhead_tip_if_good_time(&self) {
        let Some(world) = self.st.borrow().world.upgrade() else {
            // [world expertMode] on nil is NO; customRules on nil would be a zero struct (disabled -> 5 blockheads)
            return self.second_blockhead_tip(false, false, 0);
        };
        let cr = world.custom_rules();
        self.second_blockhead_tip(world.expert_mode(), cr.custom_rules_enabled != 0, cr.max_blockheads);
    }

    /// Testable core of [`display_second_blockhead_tip_if_good_time`](Self::display_second_blockhead_tip_if_good_time).
    pub fn second_blockhead_tip(&self, expert_mode: bool, custom_rules_enabled: bool, max_blockheads: i8) {
        if !expert_mode && self.st.borrow().current_tip_text.is_none() {
            let now = reference_time();
            if now - self.st.borrow().time_last_second_blockhead_tip_displayed > 1200.0f64 {
                self.st.borrow_mut().time_last_second_blockhead_tip_displayed = now;
                let max_b: i32 = if !custom_rules_enabled {
                    5
                } else if (max_blockheads as i32) < 5 {
                    max_blockheads as i32
                } else {
                    5
                };
                if max_b > 1 {
                    self.display_tip_with_time_out_display_even_if_disabled_tip_color(
                        "TIP: WARP IN ANOTHER BLOCKHEAD,\nEXPLORE & CRAFT AT THE SAME TIME!",
                        10.0,
                        false,
                        WHITE,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct B {
        name: &'static str,
        tip: Cell<i32>,
        health: f32,
        happiness: f32,
        fullness: f32,
        energy: f32,
        env: f32,
        light: f32,
        exposure: f32,
        env_temp: f32,
        cur_temp: f32,
        male: bool,
        regen: bool,
        trade: bool,
        asleep: bool,
        med: bool,
        inter: i32,
        good_bad: i32,
    }
    impl Default for B {
        fn default() -> B {
            B {
                name: "BOB",
                tip: Cell::new(0),
                health: 1.0,
                happiness: 1.0,
                fullness: 1.0,
                energy: 1.0,
                env: 1.0,
                light: 1.0,
                exposure: 1.0,
                env_temp: 1.0,
                cur_temp: 20.0,
                male: true,
                regen: false,
                trade: false,
                asleep: false,
                med: false,
                inter: 0,
                good_bad: 0,
            }
        }
    }
    impl TipBlockhead for B {
        fn name(&self) -> String {
            self.name.to_string()
        }
        fn tip_type(&self) -> i32 {
            self.tip.get()
        }
        fn set_tip_type(&self, t: i32) {
            self.tip.set(t)
        }
        fn health(&self) -> f32 {
            self.health
        }
        fn happiness(&self) -> f32 {
            self.happiness
        }
        fn fullness(&self) -> f32 {
            self.fullness
        }
        fn energy(&self) -> f32 {
            self.energy
        }
        fn environment(&self) -> f32 {
            self.env
        }
        fn environment_light(&self) -> f32 {
            self.light
        }
        fn environment_exposure(&self) -> f32 {
            self.exposure
        }
        fn environment_temperature(&self) -> f32 {
            self.env_temp
        }
        fn current_temperature(&self) -> f32 {
            self.cur_temp
        }
        fn is_male(&self) -> bool {
            self.male
        }
        fn regenerating(&self) -> bool {
            self.regen
        }
        fn on_trade_mission(&self) -> bool {
            self.trade
        }
        fn asleep(&self) -> bool {
            self.asleep
        }
        fn meditating(&self) -> bool {
            self.med
        }
        fn current_interaction_type(&self) -> i32 {
            self.inter
        }
        fn current_interaction_is_good_or_bad(&self) -> i32 {
            self.good_bad
        }
    }

    #[test]
    fn display_rules_and_timeout() {
        let t = TipManager::new_uninstanced();
        assert_eq!(t.current_tip(), None);
        assert_eq!(t.tip_color(), WHITE);
        t.display_tip_with_time_out_display_even_if_disabled_tip_color("A", 2.0, false, DAMAGE_RED);
        assert_eq!(t.current_tip().as_deref(), Some("A"));
        assert_eq!(t.tip_color(), DAMAGE_RED);
        // a different text while one is showing only changes the colour
        t.display_tip_with_time_out_display_even_if_disabled_tip_color("B", 9.0, false, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(t.current_tip().as_deref(), Some("A"));
        assert_eq!(t.tip_color(), [0.0, 1.0, 0.0, 1.0]);
        t.update(1.0);
        assert!(t.current_tip().is_some());
        // same text refreshes the timer
        t.display_tip_with_time_out_display_even_if_disabled_tip_color("A", 2.0, false, WHITE);
        t.update(1.5);
        assert!(t.current_tip().is_some());
        t.update(1.0);
        assert_eq!(t.current_tip(), None);
        // tutorial tip is the fallback
        t.set_tutorial_tip_text(Some("T"));
        assert_eq!(t.current_tip().as_deref(), Some("T"));
        t.reset();
        assert_eq!(t.current_tip(), None);
    }

    #[test]
    fn severity_zero_and_one_clear_tip_type() {
        for (ty, text) in
            [(7, "BOB IS BEING HARMED!"), (9, "BOB IS ABOUT TO DIE!"), (8, "BOB HAS DIED\nAND IS NOW REGENERATING!")]
        {
            let t = TipManager::new_uninstanced();
            let b = B::default();
            b.tip.set(ty);
            t.update_tip_for_blockhead_view(Some(&b), 0);
            assert_eq!(b.tip.get(), 0);
            assert_eq!(t.current_tip().as_deref(), Some(text));
            assert_eq!(t.tip_color(), DAMAGE_RED);
        }
        // level 0 with another type: untouched, nothing shown
        let t = TipManager::new_uninstanced();
        let b = B::default();
        b.tip.set(3);
        t.update_tip_for_blockhead_view(Some(&b), 0);
        assert_eq!(b.tip.get(), 3);
        assert_eq!(t.current_tip(), None);
        // level 1 texts
        let expect = [
            (1, "BOB COLLAPSED\nFROM EXHAUSTION."),
            (2, "BOB COLLAPSED\nFROM STARVATION."),
            (3, "BOB REFUSES TO WORK\nDUE TO EXHAUSTION."),
            (4, "BOB REFUSES TO WORK\nDUE TO STARVATION."),
            (5, "BOB IS OUT OF FUEL."),
            (6, "BOB NEEDS ELECTRICITY."),
        ];
        for (ty, text) in expect {
            let t = TipManager::new_uninstanced();
            let b = B::default();
            b.tip.set(ty);
            t.update_tip_for_blockhead_view(Some(&b), 1);
            assert_eq!(b.tip.get(), 0);
            assert_eq!(t.current_tip().as_deref(), Some(text));
            assert_eq!(t.tip_color(), WHITE);
        }
        // t = 7 at level 1 only clears
        let t = TipManager::new_uninstanced();
        let b = B::default();
        b.tip.set(7);
        t.update_tip_for_blockhead_view(Some(&b), 1);
        assert_eq!((b.tip.get(), t.current_tip()), (0, None));
        // a showing tip blocks any update (tipType kept)
        let t = TipManager::new_uninstanced();
        t.display_tip_with_time_out_display_even_if_disabled_tip_color("X", 5.0, true, WHITE);
        let b = B::default();
        b.tip.set(7);
        t.update_tip_for_blockhead_view(Some(&b), 0);
        assert_eq!(b.tip.get(), 7);
        t.update_tip_for_blockhead_view(None, 0);
    }

    #[test]
    fn severity_two_and_generic() {
        let t = TipManager::new_uninstanced();
        let mut b = B::default();
        b.health = 0.05;
        t.update_tip_for_blockhead_view(Some(&b), 2);
        assert_eq!(t.current_tip().as_deref(), Some("BOB IS BADLY INJURED,\nHEAL BY EATING FOOD."));
        // healthy: nothing
        let t = TipManager::new_uninstanced();
        t.update_tip_for_blockhead_view(Some(&B::default()), 2);
        assert_eq!(t.current_tip(), None);
        // starving, moving slowly / working slowly
        let t = TipManager::new_uninstanced();
        b = B::default();
        b.happiness = 0.0;
        b.fullness = 0.0;
        t.update_tip_for_blockhead_view(Some(&b), 2);
        assert_eq!(t.current_tip().as_deref(), Some("BOB IS MOVING SLOWLY\nBECAUSE HE IS STARVING."));
        let t = TipManager::new_uninstanced();
        b.inter = 23;
        b.male = false;
        t.update_tip_for_blockhead_view(Some(&b), 2);
        assert_eq!(t.current_tip().as_deref(), Some("BOB IS WORKING SLOWLY\nBECAUSE SHE IS STARVING."));
        // asleep: no action -> nothing
        let t = TipManager::new_uninstanced();
        b.asleep = true;
        t.update_tip_for_blockhead_view(Some(&b), 2);
        assert_eq!(t.current_tip(), None);
        // environment branches
        for (temp, light, exp, cur, text) in [
            (0.0, 1.0, 1.0, 10.0, "BECAUSE HE IS FREEZING."),
            (0.0, 1.0, 1.0, 30.0, "BECAUSE HE IS TOO HOT."),
            (1.0, 0.0, 1.0, 10.0, "BECAUSE HE CAN'T SEE."),
            (1.0, 1.0, 0.0, 10.0, "BECAUSE HE IS EXPOSED."),
        ] {
            let t = TipManager::new_uninstanced();
            let mut b = B::default();
            b.happiness = 0.0;
            b.env = 0.0;
            b.env_temp = temp;
            b.light = light;
            b.exposure = exp;
            b.cur_temp = cur;
            t.update_tip_for_blockhead_view(Some(&b), 2);
            assert_eq!(t.current_tip().as_deref(), Some(&*format!("BOB IS MOVING SLOWLY\n{text}")));
        }
        // generic branch (level 3, or regenerating)
        let t = TipManager::new_uninstanced();
        let mut b = B::default();
        b.good_bad = -1;
        t.update_tip_for_blockhead_view(Some(&b), 3);
        assert_eq!(
            t.current_tip().as_deref(),
            Some("TIP: YOU GET MORE, FASTER IF YOU\nUSE THE CORRECT TOOL FOR THE JOB.")
        );
        let t = TipManager::new_uninstanced();
        b.regen = true;
        t.update_tip_for_blockhead_view(Some(&b), 2);
        assert!(t.current_tip().is_some());
        let t = TipManager::new_uninstanced();
        b.good_bad = 1;
        t.update_tip_for_blockhead_view(Some(&b), 3);
        assert_eq!(t.current_tip(), None);
    }

    #[test]
    fn second_blockhead_tip_threshold() {
        let t = TipManager::new_uninstanced();
        // the shared clock may be virtual (tests of other modules): make "long ago" independent of its value
        t.st.borrow_mut().time_last_second_blockhead_tip_displayed = reference_time() - 2000.0;
        t.second_blockhead_tip(true, false, 5); // expert mode: nothing
        assert_eq!(t.current_tip(), None);
        t.second_blockhead_tip(false, true, 1); // maxB = 1 -> no tip (but the timestamp is taken)
        assert_eq!(t.current_tip(), None);
        t.second_blockhead_tip(false, false, 5); // within 1200 s of the last attempt
        assert_eq!(t.current_tip(), None);
        t.st.borrow_mut().time_last_second_blockhead_tip_displayed = reference_time() - 2000.0;
        t.second_blockhead_tip(false, true, 3);
        assert_eq!(
            t.current_tip().as_deref(),
            Some("TIP: WARP IN ANOTHER BLOCKHEAD,\nEXPLORE & CRAFT AT THE SAME TIME!")
        );
        assert_eq!(t.st.borrow().timeout_timer, 10.0);
    }
}
