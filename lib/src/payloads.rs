use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Set { gamma: f32 },
    Enable,
    Disable,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StatusPayload {
    pub enabled: bool,
    pub gamma_state: String,
    pub gamma: f32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Ok { message: String },
    Error { message: String },
    Status(StatusPayload),
}
