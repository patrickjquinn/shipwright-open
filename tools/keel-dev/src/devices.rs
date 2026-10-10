// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! Window presets for `keel run --device`.
//!
//! Resolutions are the phones' panel sizes in portrait. The pixel ratio is
//! the one Keel computes on a phone when the ambience gives none
//! (`keel/silica/plugin/cpp/theme.cpp`: the screen's short side over 540, in
//! quarter steps, at least 1). A phone whose dconf sets
//! `/desktop/sailfish/silica/theme_pixel_ratio` overrides it; those values
//! have not been read on the devices yet (keel/README.md, "Needs device
//! verification", step 4), so pass `--pixel-ratio` to try another.

/// A phone to imitate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Device {
    pub id: &'static str,
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
}

/// Known devices; the first is the default.
pub const DEVICES: &[Device] = &[
    Device {
        id: "default",
        name: "Keel's desktop default (SailfishApp, Jolla 1 size)",
        width: 540,
        height: 960,
    },
    Device {
        id: "jolla-1",
        name: "Jolla (2013)",
        width: 540,
        height: 960,
    },
    Device {
        id: "jolla-c",
        name: "Jolla C",
        width: 720,
        height: 1280,
    },
    Device {
        id: "jolla-c2",
        name: "Jolla C2 Community Phone",
        width: 720,
        height: 1600,
    },
    Device {
        id: "jolla-phone",
        name: "Jolla Phone (2026)",
        width: 1080,
        height: 2260,
    },
    Device {
        id: "xperia-x",
        name: "Sony Xperia X",
        width: 1080,
        height: 1920,
    },
    Device {
        id: "xperia-10-iii",
        name: "Sony Xperia 10 III",
        width: 1080,
        height: 2520,
    },
];

impl Device {
    /// Looks a device up by id.
    #[must_use]
    pub fn find(id: &str) -> Option<&'static Device> {
        DEVICES.iter().find(|d| d.id == id)
    }

    /// Keel's pixel ratio for this screen (see the module documentation).
    #[must_use]
    pub fn pixel_ratio(&self) -> f64 {
        keel_pixel_ratio(self.width.min(self.height))
    }
}

/// `max(1, round(short_side / 540 * 4) / 4)`, as Keel's Theme.
#[must_use]
pub fn keel_pixel_ratio(short_side: u32) -> f64 {
    (f64::from(short_side) / 540.0 * 4.0).round().max(4.0) / 4.0
}

/// The `--device` help text: one line per device.
#[must_use]
pub fn list() -> String {
    DEVICES
        .iter()
        .map(|d| {
            format!(
                "  {:<14} {}x{}, pixel ratio {}  {}",
                d.id,
                d.width,
                d.height,
                d.pixel_ratio(),
                d.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_follow_keel() {
        assert!((Device::find("default").unwrap().pixel_ratio() - 1.0).abs() < f64::EPSILON);
        assert!((Device::find("jolla-c2").unwrap().pixel_ratio() - 1.25).abs() < f64::EPSILON);
        assert!((Device::find("jolla-phone").unwrap().pixel_ratio() - 2.0).abs() < f64::EPSILON);
        assert!((keel_pixel_ratio(300) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ids_are_unique() {
        for (i, d) in DEVICES.iter().enumerate() {
            assert!(DEVICES[i + 1..].iter().all(|o| o.id != d.id), "{}", d.id);
        }
    }
}
