use std::error::Error;

use zbus::Connection;

const SERVICE: &str = "org.freedesktop.UPower";
const DEVICES_PATH: &str = "/org/freedesktop/UPower";
const DEVICES_INTERFACE: &str = "org.freedesktop.UPower";
const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const PROP_INTERFACE: &str = "org.freedesktop.DBus.Properties";

/// Possible battery states reported by UPower over D-Bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Unknown = 0,
    Charging = 1,
    Discharging = 2,
    Empty = 3,
    FullyCharged = 4,
    PendingCharge = 5,
    PendingDischarge = 6,
}

/// Allow safe conversion from the raw u32 received from D-Bus.
impl TryFrom<u32> for BatteryState {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(BatteryState::Unknown),
            1 => Ok(BatteryState::Charging),
            2 => Ok(BatteryState::Discharging),
            3 => Ok(BatteryState::Empty),
            4 => Ok(BatteryState::FullyCharged),
            5 => Ok(BatteryState::PendingCharge),
            6 => Ok(BatteryState::PendingDischarge),
            _ => Err(format!("Invalid UPower battery state: {}", value)),
        }
    }
}

/// Allow safe conversion directly from the D-Bus `Value` received over the wire.
impl TryFrom<zbus::zvariant::OwnedValue> for BatteryState {
    type Error = String;

    fn try_from(value: zbus::zvariant::OwnedValue) -> Result<Self, Self::Error> {
        let raw: u32 = value
            .try_into()
            .map_err(|e| format!("Expected u32 battery state from D-Bus: {}", e))?;
        raw.try_into()
    }
}

/// Information on a laptop battery: its charging state and state-of-charge.
#[derive(Debug, Clone, Copy)]
pub struct BatteryInfo {
    pub state: BatteryState,
    pub soc: f64,
}

/// A source for battery info.
pub trait BatteryProvider {
    /// Returns the current battery state and state-of-charge.
    #[allow(async_fn_in_trait)]
    async fn info(&self) -> Result<BatteryInfo, Box<dyn Error>>;
}

/// Reads battery state from UPower over a D-Bus connection.
pub struct UPower {
    conn: Connection,
    battery_path: String,
}

impl UPower {
    /// Wraps an existing D-Bus connection (typically the system bus).
    pub fn new(conn: Connection, battery_path: String) -> Self {
        Self { conn, battery_path }
    }
}

impl BatteryProvider for UPower {
    async fn info(&self) -> Result<BatteryInfo, Box<dyn Error>> {
        let state_msg = self
            .conn
            .call_method(
                Some(SERVICE),
                self.battery_path.as_str(),
                Some(PROP_INTERFACE),
                "Get",
                &(DEVICE_INTERFACE, "State"),
            )
            .await?;

        let soc_msg = self
            .conn
            .call_method(
                Some(SERVICE),
                self.battery_path.as_str(),
                Some(PROP_INTERFACE),
                "Get",
                &(DEVICE_INTERFACE, "Percentage"),
            )
            .await?;

        let state_var: zbus::zvariant::OwnedValue = state_msg.body().deserialize()?;
        let state: BatteryState = state_var.try_into()?;

        let soc_var: zbus::zvariant::OwnedValue = soc_msg.body().deserialize()?;
        let soc: f64 = soc_var.try_into()?;

        let soc_norm = soc / 100.0;

        Ok(BatteryInfo {
            state,
            soc: soc_norm,
        })
    }
}

pub async fn find_battery_path(conn: &Connection) -> Result<String, Box<dyn Error>> {
    let enumerated_devices = conn
        .call_method(
            Some(SERVICE),
            DEVICES_PATH,
            Some(DEVICES_INTERFACE),
            "EnumerateDevices",
            &(),
        )
        .await?;

    let reply: Vec<zbus::zvariant::OwnedObjectPath> = enumerated_devices.body().deserialize()?;

    let battery_path = reply
        .into_iter()
        .map(|path| path.to_string())
        .find(|s| s.contains("BAT"))
        .ok_or("Could not find battery path from dbus")?;

    Ok(battery_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{OwnedValue, Value};

    #[test]
    fn battery_state_from_each_valid_u32() {
        let cases = [
            (0u32, BatteryState::Unknown),
            (1, BatteryState::Charging),
            (2, BatteryState::Discharging),
            (3, BatteryState::Empty),
            (4, BatteryState::FullyCharged),
            (5, BatteryState::PendingCharge),
            (6, BatteryState::PendingDischarge),
        ];
        for (raw, expected) in cases {
            assert_eq!(BatteryState::try_from(raw).expect("valid state"), expected);
        }
    }

    #[test]
    fn battery_state_from_out_of_range_u32_errors() {
        let err = BatteryState::try_from(7u32).expect_err("7 is not a valid state");
        assert!(err.contains("Invalid UPower battery state"));
        assert!(err.contains('7'));
    }

    #[test]
    fn battery_state_from_owned_value_u32_succeeds() {
        let value: OwnedValue = Value::from(4u32).try_into().expect("owned u32 value");
        assert_eq!(
            BatteryState::try_from(value).expect("valid state"),
            BatteryState::FullyCharged
        );
    }

    #[test]
    fn battery_state_from_owned_value_wrong_type_errors() {
        let value: OwnedValue = Value::from("not a number")
            .try_into()
            .expect("owned str value");
        let err = BatteryState::try_from(value).expect_err("strings are not battery states");
        assert!(err.contains("Expected u32 battery state"));
    }

    #[test]
    fn battery_state_from_owned_value_out_of_range_errors() {
        let value: OwnedValue = Value::from(99u32).try_into().expect("owned u32 value");
        let err = BatteryState::try_from(value).expect_err("99 is not a valid state");
        assert!(err.contains("Invalid UPower battery state"));
    }
}
