//! The five Objective-C protocols adopted by dynamic objects, as Rust traits.
//!

use super::base::DynamicObject;
use super::handle::ObjRef;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::{IntPair, Vector2};
use crate::types::tile::Tile;

/// `PathUserDynamicObject` (Blockhead, CaveTroll and NPC family).
pub trait PathUserDynamicObject: DynamicObject {
    /// `tileIsLitForSelf:atPos:`
    fn tile_is_lit_for_self_at_pos(&self, tile: &Tile, pos: IntPair) -> bool;
    /// `currentTraverseToKeyFrame`
    fn current_traverse_to_key_frame(&self) -> i32;
    /// `nextPos`
    fn next_pos(&self) -> IntPair;
    /// `infoForPathRecalculation` (returns an `NSDictionary`)
    fn info_for_path_recalculation(&self) -> Option<PlistDict>;
    /// `setPath:type:goalInteraction:extraData:`
    fn set_path_type_goal_interaction_extra_data(
        &self,
        me: &ObjRef,
        path: Option<Vec<PlistValue>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PlistValue>,
    );
    /// `setWaitingForPathToPos:`
    fn set_waiting_for_path_to_pos(&self, p: IntPair);
    /// `setNoLongerWaitingForPath`
    fn set_no_longer_waiting_for_path(&self);
    /// `waitingForPath`
    fn waiting_for_path(&self) -> bool;
    /// `canCrawl`
    fn can_crawl(&self) -> bool;
    /// `canFly`
    fn can_fly(&self) -> bool;
    /// `pathNeedsRecalculated`
    fn path_needs_recalculated(&self) -> bool;
    /// `setPathNeedsRecalculated:`
    fn set_path_needs_recalculated(&self, v: bool);
}

/// `HarmableDynamicObject` (Blockhead, NPC family).
pub trait HarmableDynamicObject: DynamicObject {
    /// `tapIsWithinBodyRadius:`
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool;
    /// `hitWithForce:blockhead:`
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>);
    /// `willDieIfHitByForce:`
    fn will_die_if_hit_by_force(&self, force: i32) -> bool;
    /// `isVisible`
    fn is_visible(&self) -> bool;
    /// `center`
    fn center(&self) -> Vector2;
}

/// `RidableDynamicObject` (Boat, NPC mounts, TrainCar family). `renderPos`, `blockheadsLoaded` and
/// `blockheadUnloaded:` are the `DynamicObject` methods with the same selectors.
pub trait RidableDynamicObject: DynamicObject {
    /// `riderPosForBlockhead:` (a 4-lane `Vector`)
    fn rider_pos_for_blockhead(&self, b: &ObjRef) -> [f32; 4];
    /// `cameraPosForBlockhead:`
    fn camera_pos_for_blockhead(&self, b: &ObjRef) -> Vector2;
    /// `riderBodyYRotationForBlockhead:`
    fn rider_body_y_rotation_for_blockhead(&self, b: &ObjRef) -> f32;
    /// `setTargetVelocity:`
    fn set_target_velocity(&self, v: Vector2);
    /// `rideDirection`
    fn ride_direction(&self) -> i32;
    /// `addRider:`
    fn add_rider(&self, me: &ObjRef, b: &ObjRef);
    /// `removeRider:`
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef);
    /// `swipeUpGesture`
    fn swipe_up_gesture(&self, me: &ObjRef);
    /// `blockheadCanRide:usingItem:`
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, item: i32) -> bool;
    /// `jumpsOnSwipe`
    fn jumps_on_swipe(&self) -> bool;
    /// `requiresFuel`
    fn requires_fuel(&self) -> bool;
    /// `actsAsInteractionObject` (returns an object)
    fn acts_as_interaction_object(&self) -> Option<ObjRef>;
    /// `riderDPadShouldAllowUpDown`
    fn rider_d_pad_should_allow_up_down(&self) -> bool;
    /// `riderDPadShouldGiveDiscreteValues`
    fn rider_d_pad_should_give_discrete_values(&self) -> bool;
}

/// `TapChoiceDynamicObject`. `canBeRemovedByBlockhead:` is the `DynamicObject` method. InteractionObject
/// subclasses delegate to `InteractionObjectLike`.
pub trait TapChoiceDynamicObject: DynamicObject {
    /// `actionTitle`
    fn action_title(&self) -> String;
    /// `isDoubleHeight`
    fn is_double_height(&self) -> bool;
}

/// `SignTextDynamicObject` (Sign, TrainStation).
pub trait SignTextDynamicObject: DynamicObject {
    /// `text`
    fn text(&self) -> Option<String>;
    /// `setText:`
    fn set_text(&self, me: &ObjRef, t: Option<&str>);
    /// `requiresSingleLineTextEditing`
    fn requires_single_line_text_editing(&self) -> bool;
}

/// `PowerUser` (ElevatorMotor).
pub trait PowerUser: DynamicObject {
    /// `isStorageDevice`
    fn is_storage_device(&self) -> bool;
}
