use critical_section as _;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_hal_async::i2c::I2c;
use embedded_hal_mock::eh1::i2c::Transaction;
use pca9548a::*;

// #[cfg(feature = "embassy")]
#[tokio::test]
async fn main() {
    let mock_i2c = embedded_hal_mock::eh1::i2c::Mock::new(&[
        Transaction::write(BASE_ADDRESS, vec![0x01]),
        Transaction::transaction_start(BASE_ADDRESS),
        Transaction::write(0x42, vec![1, 2, 3]),
        Transaction::transaction_end(BASE_ADDRESS),
    ]);

    let pca = Pca9548a::<Mutex<CriticalSectionRawMutex, _>>::new(mock_i2c, BASE_ADDRESS);

    pca.single_subbus(0).write(0x42, &[1, 2, 3]).await.unwrap();

    pca.bus_async().await.unwrap().done();
}
