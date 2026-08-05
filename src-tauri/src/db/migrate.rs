use rusqlite::Connection;

refinery::embed_migrations!("migrations");

pub fn run(conn: &mut Connection) -> Result<(), refinery::Error> {
    migrations::runner().run(conn)?;
    Ok(())
}
