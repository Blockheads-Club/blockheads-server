//! LMDB compatibility probe: opens a Blockheads save's LMDB env (world_db, server_db,
//! lightBlocks) with heed / lmdb-master-sys and lists named DBs and keys.
//! Usage: lmdb_probe <env_dir> [--write <dbname> <key> <value>]
use heed::types::Bytes;
use heed::{Database, EnvOpenOptions};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: lmdb_probe <env_dir> [--write db key value]");
    let v = heed::lmdb_version();
    println!("linked {} ({}.{}.{})", v.string, v.major, v.minor, v.patch);
    let env = unsafe { EnvOpenOptions::new().map_size(1 << 30).max_dbs(64).open(dir)? };
    let info = env.info();
    println!("env: map_size={} last_page={} last_txnid={}", info.map_size, info.last_page_number, info.last_txn_id);
    if args.get(2).map(String::as_str) == Some("--write") {
        let (db, k, v) = (&args[3], &args[4], &args[5]);
        let mut w = env.write_txn()?;
        let d: Database<Bytes, Bytes> = env.create_database(&mut w, Some(db))?;
        d.put(&mut w, k.as_bytes(), v.as_bytes())?;
        w.commit()?;
        println!("wrote {db}/{k}");
    }
    let rtxn = env.read_txn()?;
    let main_db: Database<Bytes, Bytes> = env.open_database(&rtxn, None)?.expect("main db");
    for item in main_db.iter(&rtxn)? {
        let (name, _) = item?;
        let name = String::from_utf8_lossy(name).to_string();
        match env.open_database::<Bytes, Bytes>(&rtxn, Some(&name)) {
            Ok(Some(d)) => {
                let n = d.len(&rtxn)?;
                println!("named db {name:?}: {n} entries");
                for (i, kv) in d.iter(&rtxn)?.enumerate() {
                    let (k, v) = kv?;
                    if i < 8 {
                        let ks = if k.iter().all(|c| c.is_ascii_graphic()) {
                            format!("{:?}", String::from_utf8_lossy(k))
                        } else {
                            format!("{k:02x?}")
                        };
                        println!("   key {ks} -> {} bytes, head {:02x?}", v.len(), &v[..v.len().min(16)]);
                    }
                }
            }
            other => println!("main-db key {name:?} (not a named db: {other:?})"),
        }
    }
    Ok(())
}
