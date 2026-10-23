# server coverage group C resume
Done at 100% (lib-only, unit tests in blockheads_server so each fn is covered in one binary): trains/*, path/*, objects/*, physics_objects/*, npc/{npc,yak,donkey,donkey_like,egg*,shark,dodo}.rs
Left: npc/clown_fish.rs (~254 missed), npc/drop_bear.rs (~247), npc/scorpion.rs (~286), npc/cave_troll.rs (~630).
egg.rs: lines 103-104 (NpcEnv-less PassiveEnv::world default) and 864-866 (TestPassiveEnv::plant_type) now unhit; cover with a call in real_backed_environment / a plant_type call.
Then: full-workspace llvm-cov run + whole test suite (freight chest fix changes production toward vanilla).
Helper scripts: ~/.cache/bhcov/c-iter.sh, c-txt.sh, c-commit.sh
