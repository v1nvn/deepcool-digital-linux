//! Display module for AK400 DIGITAL SE variant
//! This variant uses a different protocol without the HID report ID prefix.

use crate::{devices::AUTO_MODE_INTERVAL, monitor::cpu::Cpu};
use super::{device_error, Mode};
use hidapi::HidApi;
use std::{thread::sleep, time::{Duration, Instant}};

pub const DEFAULT_MODE: Mode = Mode::CpuTemperature;
pub const TEMP_LIMIT_C: u8 = 90;
pub const TEMP_LIMIT_F: u8 = 194;

pub struct Display {
    cpu: Cpu,
    pub mode: Mode,
    update: Duration,
    fahrenheit: bool,
    alarm: bool,
}

impl Display {
    pub fn new(cpu: Cpu, mode: &Mode, update: Duration, fahrenheit: bool, alarm: bool) -> Self {
        // Verify the display mode
        let mode = match mode {
            Mode::Default => DEFAULT_MODE,
            Mode::Auto => Mode::Auto,
            Mode::CpuTemperature => Mode::CpuTemperature,
            Mode::CpuUsage => Mode::CpuUsage,
            _ => mode.support_error(),
        };

        Display {
            cpu,
            mode,
            update,
            fahrenheit,
            alarm,
        }
    }

    pub fn run(&self, api: &HidApi, vid: u16, pid: u16) {
        // Connect to device
        let device = api.open(vid, pid).unwrap_or_else(|_| device_error());

        // Display warning if a required module is missing
        self.cpu.warn_temp();

        // Data packet - NO report ID for SE variant
        let mut data: [u8; 64] = [0; 64];
        // Note: data[0] = 16 (report ID) is NOT sent for SE variant

        // Init sequence
        {
            let mut init_data = data.clone();
            init_data[0] = 170;  // Was data[1] in regular protocol
            device.write(&init_data).unwrap();
        }

        // Display loop
        match self.mode {
            Mode::Auto => {
                let mut initial_update = self.update;
                let mut mode = Mode::CpuTemperature;
                loop {
                    // Initial update
                    device.write(&self.status_message(&data, &mode, initial_update)).unwrap();

                    // Update until timeout
                    let timeout = Instant::now() + AUTO_MODE_INTERVAL;
                    while Instant::now() + self.update < timeout {
                        device.write(&self.status_message(&data, &mode, self.update)).unwrap();
                    }

                    // Make the next initial update faster to fit the timeframe
                    initial_update = timeout - Instant::now();

                    // Switch to the next display mode
                    mode = match mode {
                        Mode::CpuTemperature => Mode::CpuUsage,
                        Mode::CpuUsage => Mode::CpuTemperature,
                        _ => DEFAULT_MODE,
                    }
                }
            }
            _ => loop {
                device.write(&self.status_message(&data, &self.mode, self.update)).unwrap();
            }
        }
    }

    /// Reads the CPU status information and returns the data packet.
    fn status_message(&self, inital_data: &[u8; 64], mode: &Mode, update: Duration) -> [u8; 64] {
        // Clone the data packet
        let mut data = inital_data.clone();

        // Read CPU utilization
        let cpu_instant = self.cpu.read_instant();

        // Wait
        sleep(update);

        // Calculate usage & temperature
        let usage = self.cpu.get_usage(cpu_instant);
        let temp = self.cpu.get_temp(self.fahrenheit);

        // Main display - all indices shifted down by 1 (no report ID)
        match mode {
            Mode::CpuTemperature => {
                data[0] = if self.fahrenheit { 35 } else { 19 };  // Was data[1]
                data[2] = temp / 100;                              // Was data[3]
                data[3] = temp % 100 / 10;                         // Was data[4]
                data[4] = temp % 10;                               // Was data[5]
            }
            Mode::CpuUsage => {
                data[0] = 76;                                      // Was data[1]
                data[2] = usage / 100;                             // Was data[3]
                data[3] = usage % 100 / 10;                        // Was data[4]
                data[4] = usage % 10;                              // Was data[5]
            }
            _ => (),
        }
        // Status bar
        data[1] = if usage < 15 { 1 } else { (usage as f32 / 10.0).round() as u8 };  // Was data[2]
        // Alarm
        data[5] = (self.alarm && temp >= if self.fahrenheit { TEMP_LIMIT_F } else { TEMP_LIMIT_C }) as u8;  // Was data[6]

        data
    }
}
