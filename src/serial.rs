//! Serial-port discovery and connection settings for MicroPython targets.

use serde::{Deserialize, Serialize};

pub const COMMON_BAUD_RATES: &[u32] = &[
    9600, 19200, 38400, 57600, 74880, 115200, 230400, 460800, 921600,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialConfig {
    pub port: String,
    pub baud: u32,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud: 115_200,
        }
    }
}

impl SerialConfig {
    pub fn is_ready(&self) -> bool {
        !self.port.trim().is_empty() && self.baud > 0
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.port.trim().is_empty() {
            return Err("MicroPython 需要选择串口端口".to_owned());
        }
        if !(1_200..=4_000_000).contains(&self.baud) {
            return Err("串口波特率需要在 1200 到 4000000 之间".to_owned());
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        let port = self.port.trim();
        if port.is_empty() {
            "串口未选择".to_owned()
        } else {
            format!("{port} · {}", self.baud)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortChoice {
    pub port: String,
    pub label: String,
    pub score: u8,
    pub likely_esp32: bool,
}

pub fn scan() -> Result<Vec<PortChoice>, String> {
    let ports = serialport::available_ports().map_err(|err| format!("扫描串口失败：{err}"))?;
    let mut choices = ports.into_iter().map(describe).collect::<Vec<_>>();
    choices.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| port_number(&a.port).cmp(&port_number(&b.port)))
            .then_with(|| a.port.cmp(&b.port))
    });
    Ok(choices)
}

pub fn preferred_port(ports: &[PortChoice]) -> Option<&PortChoice> {
    ports.first()
}

fn describe(info: serialport::SerialPortInfo) -> PortChoice {
    let port = info.port_name.clone();
    let mut description = None;
    let mut score = 0u8;
    let mut likely_esp32 = false;

    if let serialport::SerialPortType::UsbPort(usb) = info.port_type {
        let product = usb.product.as_deref().unwrap_or_default();
        let manufacturer = usb.manufacturer.as_deref().unwrap_or_default();
        let haystack = format!("{product} {manufacturer}").to_ascii_lowercase();
        let native_esp32 =
            usb.vid == 0x303A || haystack.contains("espressif") || haystack.contains("esp32");
        let bridge = matches!(usb.vid, 0x10C4 | 0x1A86 | 0x0403 | 0x067B)
            || haystack.contains("cp210")
            || haystack.contains("ch340")
            || haystack.contains("ch341")
            || haystack.contains("ch910")
            || haystack.contains("ft232")
            || haystack.contains("pl2303")
            || haystack.contains("silicon labs");
        let usb_serial = haystack.contains("usb")
            && (haystack.contains("serial")
                || haystack.contains("uart")
                || haystack.contains("cdc"));

        if native_esp32 {
            score = 4;
            likely_esp32 = true;
        } else if bridge {
            score = 3;
            likely_esp32 = true;
        } else if usb_serial {
            score = 2;
        } else if !product.is_empty() || !manufacturer.is_empty() {
            score = 1;
        }

        let detail = if !product.is_empty() {
            product.to_owned()
        } else if !manufacturer.is_empty() {
            manufacturer.to_owned()
        } else {
            String::new()
        };
        if !detail.is_empty() {
            description = Some(detail);
        }
    }

    let label = match description {
        Some(detail) => format!("{port} · {detail}"),
        None => port.clone(),
    };

    PortChoice {
        port,
        label,
        score,
        likely_esp32,
    }
}

fn port_number(name: &str) -> u32 {
    let digits = name
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_defaults_to_115200_and_requires_a_port() {
        let serial = SerialConfig::default();
        assert_eq!(serial.baud, 115_200);
        assert!(!serial.is_ready());
        assert!(serial.validate().is_err());
    }

    #[test]
    fn serial_summary_shows_port_and_baud() {
        let serial = SerialConfig {
            port: "COM7".to_owned(),
            baud: 115_200,
        };
        assert!(serial.is_ready());
        assert!(serial.validate().is_ok());
        assert_eq!(serial.summary(), "COM7 · 115200");
    }

    #[test]
    fn sorts_com_ports_by_numeric_suffix() {
        let mut ports = [
            PortChoice {
                port: "COM10".to_owned(),
                label: "COM10".to_owned(),
                score: 0,
                likely_esp32: false,
            },
            PortChoice {
                port: "COM2".to_owned(),
                label: "COM2".to_owned(),
                score: 0,
                likely_esp32: false,
            },
        ];
        ports.sort_by_key(|port| port_number(&port.port));
        assert_eq!(ports[0].port, "COM2");
        assert_eq!(ports[1].port, "COM10");
    }

    #[test]
    fn preferred_port_is_the_first_sorted_choice() {
        let ports = vec![
            PortChoice {
                port: "COM4".to_owned(),
                label: "COM4 · ESP32".to_owned(),
                score: 4,
                likely_esp32: true,
            },
            PortChoice {
                port: "COM8".to_owned(),
                label: "COM8".to_owned(),
                score: 0,
                likely_esp32: false,
            },
        ];
        assert_eq!(
            preferred_port(&ports).map(|port| port.port.as_str()),
            Some("COM4")
        );
    }

    #[test]
    fn recognizes_espressif_and_usb_bridge_ports() {
        let native = describe(serialport::SerialPortInfo {
            port_name: "COM7".to_owned(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x303A,
                pid: 0x1001,
                serial_number: None,
                manufacturer: Some("Espressif".to_owned()),
                product: Some("USB JTAG/serial debug unit".to_owned()),
            }),
        });
        assert_eq!(native.score, 4);
        assert!(native.likely_esp32);

        let bridge = describe(serialport::SerialPortInfo {
            port_name: "COM3".to_owned(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x10C4,
                pid: 0xEA60,
                serial_number: None,
                manufacturer: Some("Silicon Labs".to_owned()),
                product: Some("CP2102 USB to UART Bridge Controller".to_owned()),
            }),
        });
        assert_eq!(bridge.score, 3);
        assert!(bridge.likely_esp32);
        assert!(bridge.label.contains("CP2102"));
    }
}
