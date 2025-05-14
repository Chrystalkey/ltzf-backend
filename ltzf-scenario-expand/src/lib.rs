use openapi::models::*;
pub mod sitzung;
pub mod vorgang;
pub enum ScenarioType{
    Vorgang,
    Sitzung
}
pub trait Scenario{
    pub type ObjectType: Send+Sync+Sized;
    pub fn load(path: &str) -> Result<Self>;
}
