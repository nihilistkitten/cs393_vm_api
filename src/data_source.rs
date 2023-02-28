pub type DsError = &'static str;

pub trait DataSource {
    // Constructors are left to each implementation, once you have one, you can:

    /// Read data from the `DataSource`.
    ///
    /// # Errors
    /// If reading fails.
    fn read(&self, offset: usize, length: usize, buffer: &mut [u8]) -> Result<(), DsError>;

    /// Write data to the `DataSource`.
    ///
    /// # Errors
    /// If writing fails.
    fn write(&self, offset: usize, length: usize, buffer: &[u8]) -> Result<(), DsError>;

    /// Flush the cache.
    ///
    /// # Errors
    /// If flushing fails.
    fn flush(&self, offset: usize, length: usize) -> Result<(), DsError>;
}
