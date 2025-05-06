See the use of the files in the tests of <../src/elf.rs> as to why those files
are relevant.

* puzzle.elf:

  Built with Rust 1.86.0 from <https://github.com/ferrous-systems/rust-exercises>
  v1.23.1 (1326c77f0cfa6b315e865f41748d500962d2e1ff) using `./build.sh` in
  ./nrf52-code/puzzle-fw/target/thumbv7em-none-eabihf/release/puzzle-fw
 
* blinky.elf:

  Built with Rust 1.86.0  from
  <https://github.com/nrf-rs/adafruit-nrf52-bluefruit-le>
  (f728ffbebe27ea54d85a3910ee4f94fe84efb627) using `cargo build`

* All .bin file were generated using from their respective .elf files using
  `arm-none-eabi-objcopy -O binary file.elf file.bin`.
