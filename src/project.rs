use crate::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn add_project(
    db: &str,
    name: &str,
    client: Option<&str>,
    hourly_rate: Option<f64>,
    color: Option<&str>,
) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let id = store.add_project(name, client, hourly_rate, color)?;
    println!("Created project '{}' (id: {})", name, id);
    Ok(())
}

pub fn list_projects(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let projects = store.list_projects()?;
    if projects.is_empty() {
        println!("No projects. Use 'trackerclaw project add <name>' to create one.");
        return Ok(());
    }
    println!(
        "{:<5} {:<20} {:<20} {:<10} COLOR",
        "ID", "NAME", "CLIENT", "RATE"
    );
    for p in projects {
        println!(
            "{:<5} {:<20} {:<20} {:<10} {}",
            p.id,
            p.name,
            p.client.as_deref().unwrap_or("-"),
            p.hourly_rate
                .map_or_else(|| "-".to_string(), |r| format!("${:.2}", r)),
            p.color.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}

pub fn edit_project(
    db: &str,
    name: &str,
    new_name: Option<&str>,
    client: Option<&str>,
    hourly_rate: Option<f64>,
    color: Option<&str>,
) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let project = store
        .get_project_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", name))?;
    store.update_project(project.id, new_name, client, hourly_rate, color)?;
    println!("Updated project '{}'.", new_name.unwrap_or(name));
    Ok(())
}

pub fn delete_project(db: &str, name: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let project = store
        .get_project_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", name))?;
    store.delete_project(project.id)?;
    println!("Deleted project '{}'.", name);
    Ok(())
}

pub fn resolve_project_id(db: &str, name: &str) -> Result<Option<i64>> {
    let store = Store::open(Path::new(db))?;
    Ok(store.get_project_by_name(name)?.map(|p| p.id))
}
