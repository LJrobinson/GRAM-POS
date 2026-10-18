//! Explicit, offline recovery into a new file; never replaces the live register.
use gram_register_service::RegisterService;
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: register-restore BACKUP.json NEW-DATABASE.sqlite3".into());
    }
    let content = std::fs::read_to_string(&args[0])?;
    RegisterService::restore_backup(&content, &args[1])?;
    println!("Recovered into the new database. The original register was not changed.");
    Ok(())
}
