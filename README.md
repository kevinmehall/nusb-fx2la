fx2la
-----

Driver for [fx2lafw](https://sigrok.org/wiki/Fx2lafw) logic analyzers with [nusb](https://github.com/kevinmehall/nusb).

## Firmware

The fx2lafw firmware is loaded to the device's RAM on first use each time the device is plugged in. By default, this library looks for the firmware in the following locations in this order:

  - `$FX2LAFW_FIRMWARE_DIR`
  - `../share/sigrok-firmware` relative to the executable
  - `$COMPILE_TIME_FX2LAFW_FIRMWARE_DIR` resolved at compile time
  - `/usr/local/share/sigrok-firmware/`
  - `/usr/share/sigrok-firmware/`

Firmware binaries can be downloaded [here](https://sigrok.org/download/binary/sigrok-firmware-fx2lafw/) or via your package manager:

### Nix (run-time)

```bash
export FX2LAFW_FIRMWARE_DIR=$(nix-build '<nixpkgs>' -A sigrok-firmware-fx2lafw --no-out-link)/share/sigrok-firmware
```

### Nix derivation (build-time)

```nix
env.COMPILE_TIME_FX2LAFW_FIRMWARE_DIR = "${pkgs.sigrok-firmware-fx2lafw}/share/sigrok-firmware";
```

### Debian / Ubuntu

```bash
sudo apt install sigrok-firmware-fx2lafw
```
