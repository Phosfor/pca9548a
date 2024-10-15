#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

use core::{future::Future, ops::DerefMut};

use embedded_hal::i2c::{ErrorType, I2c as _};
use embedded_hal_async::i2c::I2c as _;

/// The base address of a pca9548a.
///
/// You can select the effective address with the three address pins A0, A1, A2.
/// The effective address is `BASE_ADDRESS + A2 << 2 + A1 << 1 + A0 << 0`,
/// where Ax is 1 if the corresponding pin is high and 0 if it is low.
pub const BASE_ADDRESS: u8 = 0x70;

/// This crate allows using sync and async mutexes.
/// All must implement this trait.
pub trait MutexBase {
    /// The actual bus that is wrapped inside this mutex.
    type Bus;

    /// The error returned by `try_lock`
    type Error;

    /// Create a new mutex of this type.
    fn new(v: Self::Bus) -> Self;
}

/// A "normal" synchronous mutex like `std::sync::Mutex`.
///
/// If the std feature is enabled, this is implemented for `std::sync::Mutex`.
pub trait SyncMutex: MutexBase {
    /// Lock the mutex.
    fn lock(&self) -> Result<impl DerefMut<Target = Self::Bus>, Self::Error>;
}

/// An asynchronous mutex like `embassy_sync::mutex::Mutex`.
pub trait AsyncMutex: MutexBase {
    /// Lock the mutex.
    fn lock(&self) -> impl Future<Output = Result<impl DerefMut<Target = Self::Bus>, Self::Error>>;
}

#[cfg(feature = "std")]
impl<T> MutexBase for std::sync::Mutex<T> {
    type Bus = T;
    type Error = ();

    fn new(v: Self::Bus) -> Self {
        Self::new(v)
    }
}

#[cfg(feature = "std")]
impl<T> SyncMutex for std::sync::Mutex<T> {
    fn lock(&self) -> Result<impl DerefMut<Target = Self::Bus>, Self::Error> {
        self.lock().or(Err(()))
    }
}

// TODO: Untested
#[cfg(feature = "embassy")]
impl<Raw, T> MutexBase for embassy_sync::mutex::Mutex<Raw, T>
where
    Raw: embassy_sync::blocking_mutex::raw::RawMutex,
{
    type Bus = T;
    type Error = core::convert::Infallible;

    fn new(v: Self::Bus) -> Self {
        Self::new(v)
    }
}

#[cfg(feature = "embassy")]
impl<Raw, T> AsyncMutex for embassy_sync::mutex::Mutex<Raw, T>
where
    Raw: embassy_sync::blocking_mutex::raw::RawMutex,
{
    fn lock(&self) -> impl Future<Output = Result<impl DerefMut<Target = Self::Bus>, Self::Error>> {
        async { Ok(self.lock().await) }
    }
}

#[cfg(feature = "async-to-sync")]
impl<T> SyncMutex for T
where
    T: AsyncMutex,
{
    fn lock(&self) -> Result<impl DerefMut<Target = Self::Bus>, Self::Error> {
        embassy_futures::block_on(AsyncMutex::lock(self))
    }
}

/// The error type returned by most operations.
///
/// The error can either come from the mutex, or from the bus.
#[derive(Debug)]
pub enum Error<Mutex, Bus> {
    /// Mutex error
    Mutex(Mutex),
    /// Bus error
    Bus(Bus),
}

impl<Mutex, Bus> embedded_hal::i2c::Error for Error<Mutex, Bus>
where
    Mutex: core::fmt::Debug,
    Bus: embedded_hal::i2c::Error,
{
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        match self {
            Error::Mutex(_) => embedded_hal::i2c::ErrorKind::Overrun,
            Error::Bus(e) => e.kind(),
        }
    }
}

/// The Pca9548a is an i2c multiplexer device.
pub struct Pca9548a<Mutex> {
    bus: Mutex,
    address: u8,
}

impl<Mutex: MutexBase> Pca9548a<Mutex> {
    /// Create a new instance.
    pub fn new(bus: Mutex::Bus, address: u8) -> Self {
        Self {
            bus: Mutex::new(bus),
            address,
        }
    }

    /// Get a subbus from this device.
    ///
    /// * `mask` The mask to use for the subbus
    ///
    /// See [`SubBus`] for more info.
    pub fn subbus(&self, mask: u8) -> SubBus<'_, Mutex> {
        SubBus { pca: self, mask }
    }

    /// Get a subbus with a single channel enabled.
    ///
    /// * `id` The id of the subbus in range 0..=7
    ///
    /// See [`SubBus`] for more info.
    pub fn single_subbus(&self, id: u8) -> SubBus<'_, Mutex> {
        assert!(id < 8);
        self.subbus(1 << id)
    }
}

impl<Mutex: AsyncMutex> Pca9548a<Mutex> {
    /// Get a lock on the bus using an `AsyncMutex`
    pub async fn bus_async(&self) -> Result<impl DerefMut<Target = Mutex::Bus> + '_, Mutex::Error> {
        self.bus.lock().await
    }
}

impl<Mutex: SyncMutex> Pca9548a<Mutex> {
    /// Get a lock on the bus using an `SyncMutex`
    pub fn bus(&self) -> Result<impl DerefMut<Target = Mutex::Bus> + '_, Mutex::Error> {
        self.bus.lock()
    }
}

impl<Mutex: AsyncMutex> Pca9548a<Mutex>
where
    Mutex::Bus: embedded_hal_async::i2c::I2c,
{
    /// Select the subbus and return the lock to the bus.
    ///
    /// Use this version in an async context. For a non-async version see [`Self::select_mask`].
    ///
    /// * `mask` The mask to use for the subbus
    ///
    /// *Note:* You can use the returned lock, to perform your transactions on the subbus;
    /// this makes sure, that the mask is not changed by another task in the meantime.
    ///
    /// *Note:* The above guarantee only holds, if `Bus` is not a shared bus (e.g. [shared_bus](https://docs.rs/shared-bus/latest/shared_bus/)).
    pub async fn select_mask_async(
        &self,
        mask: u8,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        let mut bus = self.bus_async().await.map_err(Error::Mutex)?;
        bus.write(self.address, &[mask]).await.map_err(Error::Bus)?;
        Ok(bus)
    }

    /// Select a single subbus and return the lock to the bus.
    ///
    /// Use this version in an async context. For a non-async version see [`Self::select_single`].
    ///
    /// * `id` The id of the subbus in range 0..=7
    ///
    /// *Note:* see [`Self::select_mask_async`] for more info.
    pub async fn select_single_async(
        &self,
        id: u8,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        assert!(id < 8);
        self.select_mask_async(1 << id).await
    }
}

impl<Mutex: SyncMutex> Pca9548a<Mutex>
where
    Mutex::Bus: embedded_hal::i2c::I2c,
{
    /// Select the subbus and return the lock to the bus.
    ///
    /// Use this version in a non-async context. For a async version see [`Self::select_mask_async`].
    ///
    /// * `mask` The mask to use for the subbus
    ///
    /// *Note:* You can use the returned lock, to perform your transactions on the subbus;
    /// this makes sure, that the mask is not changed by another task in the meantime.
    ///
    /// *Note:* The above guarantee only holds, if `Bus` is not a shared bus (e.g. [shared_bus](https://docs.rs/shared-bus/latest/shared_bus/)).
    pub fn select_mask(
        &self,
        mask: u8,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        let mut bus = self.bus().map_err(Error::Mutex)?;
        bus.write(self.address, &[mask]).map_err(Error::Bus)?;
        Ok(bus)
    }

    /// Select a single subbus and return the lock to the bus.
    ///
    /// Use this version in a non-async context. For a async version see [`Self::select_single_async`].
    ///
    /// * `id` The id of the subbus in range 0..=7
    ///
    /// *Note:* see [`Self::select_mask`] for more info.
    pub fn select_single(
        &self,
        id: u8,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        assert!(id < 8);
        self.select_mask(1 << id)
    }
}

/// A proxy to a subbus.
///
/// This implements the [`embedded_hal::i2c::I2c`]/[`embedded_hal_async::i2c::I2c`] traits, so you can use it with e.g. device drivers.
///
/// Example:
/// ```no_run
/// use pca9548a::{Pca9548a, BASE_ADDRESS};
/// use embedded_hal::i2c::I2c;
///
/// let pca = Pca9548a::<std::sync::Mutex<_>>::new(i2c_bus, BASE_ADDRESS);
///
/// let mut subbus0 = pca.single_subbus(0);
///
/// subbus.write(0x42, &[1, 2, 3]).expect("write");
/// ```
pub struct SubBus<'a, Mutex> {
    pca: &'a Pca9548a<Mutex>,
    mask: u8,
}

impl<'a, Mutex> embedded_hal::i2c::ErrorType for SubBus<'a, Mutex>
where
    Mutex: MutexBase,
    Mutex::Error: core::fmt::Debug,
    Mutex::Bus: embedded_hal::i2c::ErrorType,
{
    type Error = Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>;
}

impl<'a, Mutex> SubBus<'a, Mutex>
where
    Mutex: AsyncMutex,
    Mutex::Bus: embedded_hal_async::i2c::I2c,
{
    /// Select this subbus and return the lock to the bus.
    ///
    /// Use this version in an async context. For a non-async version see [`Self::select`].
    ///
    /// *Note:* see [`Pca9548a::select_mask_async`] for more info.
    pub async fn select_async(
        &self,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        self.pca.select_mask_async(self.mask).await
    }
}

impl<'a, Mutex> embedded_hal_async::i2c::I2c for SubBus<'a, Mutex>
where
    Mutex: AsyncMutex,
    Mutex::Error: core::fmt::Debug,
    Mutex::Bus: embedded_hal_async::i2c::I2c,
{
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.select_async()
            .await?
            .transaction(address, operations)
            .await
            .map_err(Error::Bus)
    }

    // TODO: Read/Write/WriteRead
}

impl<'a, Mutex> SubBus<'a, Mutex>
where
    Mutex: SyncMutex,
    Mutex::Bus: embedded_hal::i2c::I2c,
{
    /// Select this subbus and return the lock to the bus.
    ///
    /// Use this version in a non-async context. For an async version see [`Self::select_async`].
    ///
    /// *Note:* see [`Pca9548a::select_mask`] for more info.
    pub fn select(
        &self,
    ) -> Result<
        impl DerefMut<Target = Mutex::Bus> + '_,
        Error<Mutex::Error, <Mutex::Bus as ErrorType>::Error>,
    > {
        self.pca.select_mask(self.mask)
    }
}

impl<'a, Mutex> embedded_hal::i2c::I2c for SubBus<'a, Mutex>
where
    Mutex: SyncMutex,
    Mutex::Error: core::fmt::Debug,
    Mutex::Bus: embedded_hal::i2c::I2c,
{
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.select()?
            .transaction(address, operations)
            .map_err(Error::Bus)
    }

    // TODO: Read/Write/WriteRead
}
