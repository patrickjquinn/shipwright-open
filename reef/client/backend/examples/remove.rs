// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Removes a package the way the Reef app does (`Store::remove`, with its
//! parts), for trying it on a phone: `remove <rpm name>`.

use reef_backend::packagekit::dbus::PackageKitClient;
use reef_backend::store::{InstallLedger, Store};

fn main() {
    let Some(name) = std::env::args().nth(1) else {
        eprintln!("usage: remove <rpm name>");
        std::process::exit(2);
    };
    let result = futures_lite::future::block_on(async {
        let pm = PackageKitClient::system().await?;
        let store = Store::new(pm, "shipwright-reef")
            .with_ledger(InstallLedger::default_path().map(InstallLedger::new));
        store
            .remove(&name, &mut |p| eprintln!("{:?} {:?}", p.status, p.percentage))
            .await
    });
    match result {
        Ok(()) => println!("removed {name}"),
        Err(e) => {
            println!("failed: {e}");
            std::process::exit(1);
        }
    }
}
