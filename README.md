fx2la
-----

[Documentation](https://docs.rs/fx2la) | [Releases](https://github.com/kevinmehall/nusb-fx2la/releases)

Driver for [fx2lafw](https://sigrok.org/wiki/Fx2lafw) logic analyzers with [nusb](https://github.com/kevinmehall/nusb).

## Firmware

The fx2lafw firmware is loaded to the device's RAM on first use each time the device is plugged in. The default firmware provider looks for firmware in common filesystem locations as well as the directories specified by the environment variables `$FX2LAFW_FIRMWARE_DIR` (at runtime) and `$COMPILE_TIME_FX2LAFW_FIRMWARE_DIR` (at compile time).

Firmware binaries can be downloaded [from the Sigrok project](https://sigrok.org/download/binary/sigrok-firmware-fx2lafw/) or via your package manager:

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
