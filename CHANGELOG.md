# Changelog

## 0.2.0

- Added `--help` argument
- Added `--serial xx` argument to pick a specific dongle
- Added `--abort` argument to take a dongle out of DFU mode
- Added `--all` argument to act on all discovered dongles
- Added `--get-images` to list what is on the dongle(s)
- Upgraded to latest `serialport` library

## 0.1.3

- Relax firmware address check to support development boards other than the nRF-Dongle

## 0.1.2

- Improve error messages
- Change crc32 dependency

## 0.1.1

- Fixed timeouts on Windows

## 0.1.0

- Initial release
