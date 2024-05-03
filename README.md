# PCA9548A Driver

PCA9548a I2C-Expander driver using embedded-hal.

## Simple Usage
You can select one ore more channels to activate.
```rust
use pca9548a::{Pca9548a, BASE_ADDRESS};
use embedded_hal::i2c::I2c;

let pca = Pca9548a::<std::sync::Mutex<_>>::new(i2c_bus, BASE_ADDRESS);

// `select_*()` returns an i2c-bus that can be used to perform transactions.
pca.select_single(0).unwrap().write(0x42, &[1, 2]).unwrap();
pca.select_mask(1 << 2 | 1 << 3).unwrap().write(0x42, &[1, 2]).unwrap();
```

## SubBus
Often you will want to use device drivers that expect to take ownership of a type implementing `I2c`.

For this a proxy type `SubBus` is provided that implements [`embedded_hal::i2c::I2c`]/[`embedded_hal_async::i2c::I2c`] (depending on the underlying mutex and i2c bus implementations).

You can even use it to cascade PCA9548As:
```rust
use pca9548a::{Pca9548a, BASE_ADDRESS};

let pca = Pca9548a::<std::sync::Mutex<_>>::new(i2c_bus, BASE_ADDRESS);
// NOTE: If you chain PCA9548As, they must have different addresses
let pca_0 = Pca9548a::<std::sync::Mutex<_>>::new(pca.single_subbus(0), BASE_ADDRESS + 1);
let pca_1 = Pca9548a::<std::sync::Mutex<_>>::new(pca.single_subbus(1), BASE_ADDRESS + 1);
let pca_1_7 = Pca9548a::<std::sync::Mutex<_>>::new(pca_1.single_subbus(7), BASE_ADDRESS + 2);

// This will correctly select the channel 1 on pca, then channel 7 on pca_1, then channel 3 on pca_1_7
// before writing [1, 2] to address 0x42.
pca_1_7.select_single(3).unwrap().write(0x42, &[1, 2]).unwrap();
```

## Note on SharedBus
This driver requires shared access to the underlying i2c bus similar to the `shared_bus` crate.
A mutex is used to implement this.

When selecting a set of channels, a lock to the i2c-bus is returned; this can be used to perform transactions and prevents other tasks changing the selection before you are done.

However, this assumes that the `Pca9548a` struct has **exclusive** access to the underlying i2c bus. If you use `shared_bus` or similar, you must make sure that other tasks cannot change the selection, as this may break the guarantee that the correct channels are selected.
