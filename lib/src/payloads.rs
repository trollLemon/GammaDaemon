use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct StatusPayload {
    pub enabled: bool,
    pub gamma_state: String,
    pub gamma: u8,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TogglePayload {
    pub action: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorPayload {
    pub status: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SetPayload {
    pub gamma: u8,
}
