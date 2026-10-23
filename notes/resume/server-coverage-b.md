# server coverage group B resume (branch test/coverage-b)
Helpers: ~/.cache/bhcov/b/{iter.sh,qt.sh,fmt.sh,show.sh,addtests.py}. All tests are server-crate unit tests (single binary).
Done at 100% (last run): items/*, crafting/*, interaction/*, blockhead/inventory_ops.rs.
Near done: action.rs(1), action_queue.rs(9), ai.rs(33), blockhead.rs(31), lifecycle.rs(6), inventory_model.rs (host-stub lines 100-101 etc), inventory_add.rs (~40 lines: warp-check tail, phase 3 target branches, ownership fallbacks; tests just added, re-measure).
Untouched: state_changes.rs (321 missed), interaction_rules.rs (696), update.rs (993).
Next: iter.sh "" blockhead/ then target the remaining ranges.
