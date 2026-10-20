//! Trade price multipliers: the `worldPrices` file, the 0x2e transaction handler and the 0x2f broadcast.
//! There is no price HTTP client (R8: `getPricesConnection` is never assigned in the original).
//!

use crate::fmath::libm::pow;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::plist::helpers::{data_from_property_list, property_list_from_data, property_list_from_data_with_options};
use crate::plist::value::PlistValue;
use crate::world::lifecycle::{new_mutable_dict, save_dir_for, write_file_atomically};
use crate::world::state::World;

/// `BHNDT_SERVER_WORLD_PRICE_OFFSET_LIST`
const BHNDT_SERVER_WORLD_PRICE_OFFSET_LIST: u8 = 0x2f;

impl World {
    fn world_prices_path(&self) -> String {
        format!("{}/worldPrices", save_dir_for(&self.save_id.borrow().clone().unwrap_or_default()))
    }

    /// The `saveQueue` write block (`__59` / `__44`): XML plist of `worldPriceMultipliers` to
    /// `saves/<id>/worldPrices` atomically; skipped when serialisation gives nil. Runs synchronously.
    fn write_world_prices(&self) {
        let data = {
            let d = self.world_price_multipliers.borrow();
            d.as_ref().and_then(|d| data_from_property_list(&PlistValue::Dict(d.clone())))
        };
        if let Some(bytes) = data {
            let _ = write_file_atomically(&self.world_prices_path(), &bytes);
        }
    }

    /// `-[World updateTradePricesIfNeeded]`: when `worldPriceMultipliers` is nil, read `saves/<id>/worldPrices`
    /// (mutable containers, options 1); a missing / unparsable / non-dictionary file gives an empty dictionary.
    pub fn update_trade_prices_if_needed(&self) {
        if self.world_price_multipliers.borrow().is_some() {
            return;
        }
        let mut loaded = None;
        if let Ok(bytes) = std::fs::read(self.world_prices_path()) {
            if let Some(PlistValue::Dict(d)) = property_list_from_data_with_options(&bytes, 1) {
                loaded = Some(d);
            }
        }
        *self.world_price_multipliers.borrow_mut() = Some(loaded.unwrap_or_else(new_mutable_dict));
    }

    /// `-[World updatePriceForItemBoughtOrSoldOfTypeKey:soldCount:]`: for `|count| >= 0.01` multiply the key's
    /// multiplier by `pow(0.999, count)` (f64) and rewrite `worldPrices`; accumulate the unsent transactions as f32.
    /// Positive count = the player SOLD to the portal (R9). Boundary is `>=`/`<=` here (`>` in 0x2e).
    pub fn update_price_for_item_bought_or_sold_of_type_key_sold_count(&self, key: &str, count: f32) {
        let c = count as f64;
        if c >= 0.01 || c <= -0.01 {
            // client == nil
            {
                let mut guard = self.world_price_multipliers.borrow_mut();
                if let Some(dict) = guard.as_mut() {
                    let cur = dict.map.get(&key.to_string()).map_or(1.0, |v| v.double_value());
                    dict.map.insert(key.to_string(), PlistValue::real_f64(pow(0.999, c) * cur));
                }
            }
            self.write_world_prices();
            for slot in [&self.unsent_global_trade_transactions, &self.unsent_multiplayer_trade_transactions] {
                let mut g = slot.borrow_mut();
                let d = g.get_or_insert_with(new_mutable_dict);
                let v: f32 = match d.map.get(&key.to_string()) {
                    Some(old) => old.float_value() + count,
                    None => count,
                };
                d.map.insert(key.to_string(), PlistValue::real_f32(v));
            }
        }
    }

    /// `-[World transactionDataRecieved:fromClient:]`: handler for 0x2e (`{itemKey: soldCount}`). For each key (GsDict
    /// order) with `count > 0.01 || count < -0.01` (strict) update the multiplier and record the FULL new multiplier
    /// in the offsets; then ALWAYS (even when empty) broadcast `0x2f + gzip(plist(offsets))` reliably to all
    /// connected clients and rewrite the file. A nil dictionary leaves the loop empty.
    pub fn transaction_data_recieved_from_client(&self, transaction_data: Option<&PlistValue>, _client_id: &str) {
        let mut offsets = new_mutable_dict();
        if let Some(PlistValue::Dict(td)) = transaction_data {
            for key in td.map.keys_in_order() {
                let count: f32 = td.map.get(&key).map_or(0.0, |v| v.float_value());
                let c = count as f64;
                if c > 0.01 || c < -0.01 {
                    let mut guard = self.world_price_multipliers.borrow_mut();
                    let cur = guard.as_ref().and_then(|d| d.map.get(&key)).map_or(1.0, |v| v.double_value());
                    let new = pow(0.999, c) * cur;
                    if let Some(dict) = guard.as_mut() {
                        dict.map.insert(key.clone(), PlistValue::real_f64(new));
                    }
                    offsets.map.insert(key, PlistValue::real_f64(new));
                }
            }
        }
        let mut pkt = vec![BHNDT_SERVER_WORLD_PRICE_OFFSET_LIST];
        if let Some(x) = data_from_property_list(&PlistValue::Dict(offsets)) {
            pkt.extend_from_slice(&gzip_deflate(&x));
        }
        self.send_to_peers(&pkt, None, true);
        self.write_world_prices();
    }

    /// `-[World worldPriceOffsetsRecievedFromServer:]`: client receive of 0x2f; kept for completeness (values copied
    /// as is into `worldPriceMultipliers`, which must exist).
    pub fn world_price_offsets_recieved_from_server(&self, price_data: &[u8]) {
        let dict = gzip_inflate(price_data).and_then(|b| property_list_from_data(&b));
        if let Some(PlistValue::Dict(d)) = dict {
            let mut guard = self.world_price_multipliers.borrow_mut();
            if let Some(m) = guard.as_mut() {
                for (k, v) in d.map.iter() {
                    m.map.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::lifecycle::testkit::*;

    fn sandbox(tag: &str) -> std::sync::MutexGuard<'static, ()> {
        let g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhwc_{}_{}", std::env::temp_dir().display(), tag, std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv", home)).unwrap();
        crate::foundation::paths::set_home_for_test(&home);
        g
    }

    fn mult(w: &World, k: &str) -> Option<f64> {
        w.world_price_multipliers.borrow().as_ref().and_then(|d| d.map.get(&k.to_string())).map(|v| v.double_value())
    }

    #[test]
    fn update_price_boundary_is_inclusive_and_file_is_rewritten() {
        let _g = sandbox("prices1");
        let f = fixture(32);
        let w = &f.world;
        w.update_trade_prices_if_needed();
        assert!(w.world_price_multipliers.borrow().as_ref().unwrap().map.is_empty());
        w.update_price_for_item_bought_or_sold_of_type_key_sold_count("298", 0.005);
        assert_eq!(mult(w, "298"), None);
        w.update_price_for_item_bought_or_sold_of_type_key_sold_count("298", 1.0); // >= 0.01 and the usage multiplier
        assert_eq!(mult(w, "298"), Some(pow(0.999, 1.0)));
        w.update_price_for_item_bought_or_sold_of_type_key_sold_count("298", -1.0); // bought: the multiplier rises again
        assert_eq!(mult(w, "298"), Some(pow(0.999, -1.0) * pow(0.999, 1.0)));
        // the argument is a float: (double)0.01f is 0.0099999998 and falls below the threshold, 0.0100001f is above
        w.update_price_for_item_bought_or_sold_of_type_key_sold_count("8", 0.01f32);
        assert!(mult(w, "8").is_none());
        w.update_price_for_item_bought_or_sold_of_type_key_sold_count("7", 0.0100001f32);
        assert!(mult(w, "7").is_some());
        // unsent transactions accumulate as f32
        let unsent = w.unsent_global_trade_transactions.borrow().clone().unwrap();
        assert_eq!(unsent.map.get(&"298".to_string()).unwrap().float_value(), 0.0);
        assert_eq!(
            w.unsent_multiplayer_trade_transactions
                .borrow()
                .as_ref()
                .unwrap()
                .map
                .get(&"7".to_string())
                .unwrap()
                .float_value(),
            0.0100001f32
        );
        // file content reloaded by a new world
        let f2 = fixture(32);
        f2.world.update_trade_prices_if_needed();
        let got = mult(&f2.world, "298").unwrap();
        assert!((got - mult(w, "298").unwrap()).abs() < 1e-9);
        assert!(f.rec.sends.borrow().is_empty(), "no packet from the server-side update");
    }

    #[test]
    fn transaction_handler_uses_strict_thresholds_and_always_broadcasts() {
        let _g = sandbox("prices2");
        let f = fixture(32);
        let w = &f.world;
        w.update_trade_prices_if_needed();
        let mut td = new_mutable_dict();
        td.map.insert("298".into(), PlistValue::real_f32(3.0));
        td.map.insert("5".into(), PlistValue::real_f32(0.01)); // not strictly greater (as double 0.0099999998 < 0.01)
        td.map.insert("6".into(), PlistValue::real_f32(-3.0));
        w.transaction_data_recieved_from_client(Some(&PlistValue::Dict(td)), "c");
        assert_eq!(mult(w, "298"), Some(pow(0.999, 3.0)));
        assert_eq!(mult(w, "6"), Some(pow(0.999, -3.0)));
        assert_eq!(mult(w, "5"), None);
        let sends = f.rec.sends.borrow();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].0[0], 0x2f);
        assert!(sends[0].1.is_none() && sends[0].2);
        let d = property_list_from_data(&gzip_inflate(&sends[0].0[1..]).unwrap()).unwrap();
        assert!(
            (d.object_for_key("298").unwrap().double_value() - pow(0.999, 3.0)).abs() < 1e-12,
            "XML text quantisation of the plist writer"
        );
        assert!(d.object_for_key("5").is_none());
        drop(sends);
        // an empty offsets dictionary is still broadcast; nil dictionary too
        w.transaction_data_recieved_from_client(Some(&PlistValue::Dict(new_mutable_dict())), "c");
        w.transaction_data_recieved_from_client(None, "c");
        let sends = f.rec.sends.borrow();
        assert_eq!(sends.len(), 3);
        let d = property_list_from_data(&gzip_inflate(&sends[2].0[1..]).unwrap()).unwrap();
        assert!(matches!(d, PlistValue::Dict(ref m) if m.map.is_empty()));
        // unsent dictionaries are not touched by 0x2e
        assert!(w.unsent_global_trade_transactions.borrow().is_none());
    }

    #[test]
    fn multipliers_compound_and_client_receive_overwrites() {
        let _g = sandbox("prices3");
        let f = fixture(32);
        let w = &f.world;
        w.update_trade_prices_if_needed();
        let mut td = new_mutable_dict();
        td.map.insert("1".into(), PlistValue::real_f32(2.0));
        w.transaction_data_recieved_from_client(Some(&PlistValue::Dict(td.clone())), "c");
        w.transaction_data_recieved_from_client(Some(&PlistValue::Dict(td)), "c");
        assert_eq!(mult(w, "1"), Some(pow(0.999, 2.0) * pow(0.999, 2.0)));
        let mut off = new_mutable_dict();
        off.map.insert("1".into(), PlistValue::real_f64(0.5));
        let bytes = gzip_deflate(&data_from_property_list(&PlistValue::Dict(off)).unwrap());
        w.world_price_offsets_recieved_from_server(&bytes);
        assert_eq!(mult(w, "1"), Some(0.5));
    }
}
