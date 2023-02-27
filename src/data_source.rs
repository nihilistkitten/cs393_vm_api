use std::fs;

pub trait DataSource {
    // Constructors are left to each implementation, once you have one, you can:

    /// Read data from the `DataSource`.
    ///
    /// # Errors
    /// If reading fails.
    fn read(&self, offset: usize, length: usize, buffer: &mut [u8]) -> Result<(), &str>;

    /// Write data to the `DataSource`.
    ///
    /// # Errors
    /// If writing fails.
    fn write(&self, offset: usize, length: usize, buffer: &[u8]) -> Result<(), &str>;

    /// Flush the cache.
    ///
    /// # Errors
    /// If flushing fails.
    fn flush(&self, offset: usize, length: usize) -> Result<(), &str>;
}

pub struct File {
    file_handle: fs::File,
    name: String,
}

impl File {
    /// Create a new `File`.
    ///
    /// # Errors
    /// If the file can't be opened.
    pub fn new(name: &str) -> Result<Self, &str> {
        fs::File::open(name).map_or(Err("couldn't open {name}"), |file_handle| {
            Ok(Self {
                file_handle,
                name: name.to_string(),
            })
        })
    }
}

impl DataSource for File {
    fn read(&self, offset: usize, length: usize, buffer: &mut [u8]) -> Result<(), &str> {
        todo!()
    }
    fn write(&self, offset: usize, length: usize, buffer: &[u8]) -> Result<(), &str> {
        todo!()
    }
    fn flush(&self, offset: usize, length: usize) -> Result<(), &str> {
        todo!()
    }
}
