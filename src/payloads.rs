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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_status_serializes_with_snake_case_cmd_tag() {
        let json = serde_json::to_string(&Request::Status).expect("serialize");
        assert_eq!(json, r#"{"cmd":"status"}"#);
    }

    #[test]
    fn request_set_serializes_with_gamma_field() {
        let json = serde_json::to_string(&Request::Set { gamma: 0.5 }).expect("serialize");
        assert_eq!(json, r#"{"cmd":"set","gamma":0.5}"#);
    }

    #[test]
    fn request_deserializes_from_snake_case_cmd_tag() {
        let enable: Request = serde_json::from_str(r#"{"cmd":"enable"}"#).expect("deserialize");
        assert!(matches!(enable, Request::Enable));

        let disable: Request = serde_json::from_str(r#"{"cmd":"disable"}"#).expect("deserialize");
        assert!(matches!(disable, Request::Disable));

        let set: Request =
            serde_json::from_str(r#"{"cmd":"set","gamma":0.25}"#).expect("deserialize");
        assert!(matches!(set, Request::Set { gamma } if gamma == 0.25));
    }

    #[test]
    fn request_with_unknown_cmd_fails_to_deserialize() {
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"reboot"}"#).is_err());
    }

    #[test]
    fn request_set_without_gamma_fails_to_deserialize() {
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"set"}"#).is_err());
    }

    #[test]
    fn request_set_with_non_numeric_gamma_fails_to_deserialize() {
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"set","gamma":"bright"}"#).is_err());
    }

    #[test]
    fn request_without_cmd_tag_fails_to_deserialize() {
        assert!(serde_json::from_str::<Request>(r#"{"gamma":0.5}"#).is_err());
    }

    #[test]
    fn response_status_serializes_with_flattened_kind_tag() {
        let json = serde_json::to_string(&Response::Status(StatusPayload {
            enabled: true,
            gamma_state: "Gamma Full".to_string(),
            gamma: 1.0,
        }))
        .expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"status","enabled":true,"gamma_state":"Gamma Full","gamma":1.0}"#
        );
    }

    #[test]
    fn response_ok_and_error_round_trip() {
        for response in [
            Response::Ok {
                message: "done".to_string(),
            },
            Response::Error {
                message: "boom".to_string(),
            },
        ] {
            let json = serde_json::to_string(&response).expect("serialize");
            let decoded: Response = serde_json::from_str(&json).expect("deserialize");
            match (response, decoded) {
                (Response::Ok { message: a }, Response::Ok { message: b }) => assert_eq!(a, b),
                (Response::Error { message: a }, Response::Error { message: b }) => {
                    assert_eq!(a, b)
                }
                other => panic!("response variant changed across round trip: {other:?}"),
            }
        }
    }

    #[test]
    fn response_status_round_trips_preserving_payload() {
        let json = serde_json::to_string(&Response::Status(StatusPayload {
            enabled: false,
            gamma_state: "Gamma Low".to_string(),
            gamma: 0.4,
        }))
        .expect("serialize");
        let decoded: Response = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            Response::Status(status) => {
                assert!(!status.enabled);
                assert_eq!(status.gamma_state, "Gamma Low");
                assert_eq!(status.gamma, 0.4);
            }
            other => panic!("expected status response, got {other:?}"),
        }
    }
}
