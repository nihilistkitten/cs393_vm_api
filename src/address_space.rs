use crate::data_source::DataSource;
use scapegoat::SgSet;

#[cfg(test)]
// Use std for testing only.
extern crate std;

pub const DEFAULT_PAGE_SIZE: usize = 4096;
pub const VADDR_MAX: usize = (1 << 38) - 1;

type VirtualAddress = usize;
type AsError = &'static str;

// ?Sized is OK: we only store &D, which is Sized.
#[derive(Default)]
struct MapEntry<'a> {
    addr: usize,
    length: usize,
    // Needs to be `Option` so we can implement `Default`, required for the `SgSet` API.
    source: Option<&'a dyn DataSource>,
}

#[cfg(test)]
impl std::fmt::Debug for MapEntry<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} -> {}: {} bit mapping",
            self.addr,
            self.end(),
            self.length
        )
    }
}

impl MapEntry<'_> {
    const fn end(&self) -> usize {
        self.addr + self.length
    }
}

impl PartialEq for MapEntry<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr && self.length == other.length
    }
}

impl Eq for MapEntry<'_> {}

impl PartialOrd for MapEntry<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.addr.cmp(&other.addr))
    }
}

impl Ord for MapEntry<'_> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.addr.cmp(&other.addr)
    }
}

/// An address space.
pub struct AddressSpace<
    'a,
    const N_PAGES: usize,
    const PAGE_SIZE: usize = DEFAULT_PAGE_SIZE,
    const MIN_GAP_SIZE: usize = PAGE_SIZE,
> {
    name: &'a str,
    mappings: SgSet<MapEntry<'a>, N_PAGES>,
}

#[cfg(test)]
impl<const N_PAGES: usize, const PAGE_SIZE: usize, const MIN_GAP_SIZE: usize> std::fmt::Debug
    for AddressSpace<'_, N_PAGES, PAGE_SIZE, MIN_GAP_SIZE>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{}", self.name)?;
        for m in self.mappings.iter() {
            writeln!(f, "{m:?}")?;
        }
        Ok(())
    }
}

impl<'a, const N_PAGES: usize, const PAGE_SIZE: usize, const MIN_GAP_SIZE: usize>
    AddressSpace<'a, N_PAGES, PAGE_SIZE, MIN_GAP_SIZE>
{
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            mappings: SgSet::new(),
        }
    }

    const fn total_capacity() -> usize {
        N_PAGES * PAGE_SIZE
    }

    // Check if there is space after addr for a page of the given length, before the given address.
    fn is_space_before(target: VirtualAddress, length: usize, bound: VirtualAddress) -> bool {
        // first ensure subtraction won't overflow
        #[cfg(test)]
        {
            std::dbg!(bound);
            std::dbg!(target);
            std::dbg!(length);
        }
        bound > target + length && bound - target - length > MIN_GAP_SIZE
    }

    /// Check if there is space at addr.
    fn is_space_at(&self, addr: VirtualAddress, length: usize) -> bool {
        // Find the first mapping ending after addr, and check that there is enough space before it.
        let bound = self
            .mappings
            .iter()
            .find(|m| m.end() > addr)
            // If we don't find anything, then no mapping ends after `addr`, so we need to check
            // that there's room before the end of the file.
            .map_or(Self::total_capacity(), |m| m.addr);

        Self::is_space_before(addr, length, bound)
    }

    /// Find the space for a page of the given length.
    fn find_space_for(&self, length: usize) -> Option<VirtualAddress> {
        // TODO: perf
        //
        // Search the following addresses:
        //
        // 1. PAGE_SIZE, i.e., the location of the first possible page, since we never allocate
        //    page 0.
        // 2. For each preexisting mapping, the smallest PAGE_SIZE-aligned address which is at
        //    least MIN_GAP_SIZE larger than the mapping.
        core::iter::once(PAGE_SIZE)
            .chain(
                self.mappings
                    .iter()
                    .map(|m| (m.end() + MIN_GAP_SIZE).next_multiple_of(PAGE_SIZE)),
            )
            .find(|&a| self.is_space_at(a, length))
    }

    /// An _expensive_ check to ensure that the `AddressSpace` is in a valid state, i.e.:
    ///  * The zero page is free.
    ///  * No mappings overlap.
    ///  * There is at least `MIN_GAP_SIZE` space between each mapping.
    ///  * All mappings are `PAGE_SIZE`-aligned.
    fn assert_valid(&self) {
        // The zero page is free.
        assert!(!self.mappings.iter().any(|m| m.addr < PAGE_SIZE));

        let iter_1 = self.mappings.iter();
        let iter_2 = self.mappings.iter().skip(1);

        for (m1, m2) in iter_1.zip(iter_2) {
            // mappings.iter is in-order, so here we're guaranteed:
            // m1.addr <= m2.addr
            // there is no m3 s.t. m1.addr < m3.addr < m2.addr
            assert!(m1.end() + MIN_GAP_SIZE <= m2.addr);
        }

        for m in self.mappings.iter() {
            assert!(m.addr % PAGE_SIZE == 0);
        }
    }

    /// Add a mapping from a `DataSource` into this `AddressSpace`.
    ///
    /// # Errors
    /// If the desired mapping is invalid.
    ///
    /// # Panics
    /// todo
    pub fn add_mapping<D: DataSource>(
        &mut self,
        source: &'a D,
        length: usize,
    ) -> Result<VirtualAddress, AsError> {
        let addr = self.find_space_for(length).ok_or("no space available")?;
        self.mappings.insert(MapEntry {
            addr,
            length,
            source: Some(source),
        });
        Ok(addr)
    }

    /// Add a mapping from `DataSource` into this `AddressSpace` starting at a specific address.
    ///
    /// # Errors
    /// If there is insufficient room subsequent to `start`.
    ///
    /// # Panics
    /// todo
    pub fn add_mapping_at<D: DataSource>(
        &self,
        source: &D,
        offset: usize,
        length: usize,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        todo!()
    }

    /// Remove the mapping to `DataSource` that starts at the given address.
    ///
    /// # Errors
    /// If the mapping could not be removed.
    ///
    /// # Panics
    /// todo
    pub fn remove_mapping<D: DataSource>(
        &self,
        source: &D,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        todo!()
    }

    /// Look up the `DataSource` and offset within that `DataSource` for a
    /// `VirtualAddress` / `AccessType` in this `AddressSpace`
    ///
    /// # Errors
    /// If this `VirtualAddress` does not have a valid mapping in &self,
    /// or if this `AccessType` is not permitted by the mapping
    ///
    /// # Panics
    /// todo
    pub fn get_source_for_addr<D: DataSource>(
        &self,
        addr: VirtualAddress,
        access_type: Flags,
    ) -> Result<(&D, usize), &str> {
        todo!();
    }
}

// Visibility boundary to ensure private internals, so our validation scheme works properly.
mod flags {
    /// Build flags for address space maps.
    ///
    /// You should prefer the `flags` macro for constant-time creation of flags; this type is only used
    /// for dynamic creation.
    ///
    /// We recommend using this builder type as follows:
    /// ```
    /// # use reedos_address_space::Flags;
    /// let flags = Flags::build()
    ///     .toggle_read()
    ///     .toggle_write();
    /// ```
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    #[allow(clippy::struct_excessive_bools)] // clippy is wrong: bools are more readable than enums
                                             // here because these directly correspond to yes/no
                                             // hardware flags
    pub struct FlagBuilder {
        pub read: bool,
        pub write: bool,
        pub execute: bool,
        pub cow: bool,
        pub private: bool,
        pub shared: bool,
    }

    /// Create a toggler for a `FlagBuilder` field.
    macro_rules! flag_toggle {
        (
        $flag:ident,
        $toggle:ident,
        $setter:ident
    ) => {
            #[doc=concat!("Toggle the ", stringify!($flag), " flag.")]
            #[must_use]
            pub const fn $toggle(self) -> Self {
                Self {
                    $flag: !self.$flag,
                    ..self
                }
            }

            #[doc=concat!("Set the ", stringify!($flag), " flag.")]
            #[must_use]
            pub const fn $setter(self, to: bool) -> Self {
                Self { $flag: to, ..self }
            }
        };
    }

    impl FlagBuilder {
        /// Create a new `FlagBuilder` with all flags toggled off.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Validate that the `FlagBuilder` represents valid flags.
        ///
        /// # Panics
        /// If the `FlagBuilder` represents invalid flags.
        #[must_use]
        pub fn validate(self) -> Flags {
            assert!(!(self.private && self.shared));
            Flags {
                read: self.read,
                write: self.write,
                execute: self.execute,
                cow: self.cow,
                private: self.private,
                shared: self.shared,
            }
        }

        flag_toggle!(read, toggle_read, set_read);
        flag_toggle!(write, toggle_write, set_write);
        flag_toggle!(execute, toggle_execute, set_execute);
        flag_toggle!(cow, toggle_cow, set_cow);
        flag_toggle!(private, toggle_private, set_private);
        flag_toggle!(shared, toggle_shared, set_shared);

        #[must_use]
        /// Combine two `FlagBuilder`s by boolean or-ing each of their flags.
        ///
        /// This is, somewhat counter-intuitively, named `and`, so that the following code reads
        /// correctly:
        ///
        /// ```
        /// # use reedos_address_space::Flags;
        /// let read = Flags::read();
        /// let execute = Flags::execute();
        /// let new = read.and(execute);
        /// assert_eq!(new, Flags::build().toggle_read().toggle_execute());
        /// ```
        pub const fn and(self, other: Self) -> Self {
            let read = self.read || other.read;
            let write = self.write || other.write;
            let execute = self.execute || other.execute;
            let cow = self.cow || other.cow;
            let private = self.private || other.private;
            let shared = self.shared || other.shared;

            Self {
                read,
                write,
                execute,
                cow,
                private,
                shared,
            }
        }

        #[must_use]
        /// Turn off all flags in self that are on in other.
        ///
        /// You can think of this as `self &! other` on each field.
        ///
        /// ```
        /// # use reedos_address_space::Flags;
        /// let read_execute = Flags::read().toggle_execute();
        /// let execute = Flags::execute();
        /// let new = read_execute.but_not(execute);
        /// assert_eq!(new, Flags::build().toggle_read());
        /// ```
        pub const fn but_not(self, other: Self) -> Self {
            let read = self.read && !other.read;
            let write = self.write && !other.write;
            let execute = self.execute && !other.execute;
            let cow = self.cow && !other.cow;
            let private = self.private && !other.private;
            let shared = self.shared && !other.shared;

            Self {
                read,
                write,
                execute,
                cow,
                private,
                shared,
            }
        }
    }

    /// Create a constructor for a `Flags` object.
    macro_rules! flag_constructor {
        (
        $flag:ident
    ) => {
            #[doc=concat!("Turn on only the ", stringify!($flag), " flag.")]
            #[must_use]
            pub fn $flag() -> FlagBuilder {
                FlagBuilder {
                    $flag: true,
                    ..FlagBuilder::default()
                }
            }
        };
    }

    /// Access flags for virtual memory.
    ///
    /// There are two ways to create a `Flags`:
    ///
    /// 1. The `FlagBuilder` type, in particular `Flags::build`, which has public fields and allows
    ///    dynamic creation of flags.
    /// 2. The `flags` macro.
    #[allow(clippy::struct_excessive_bools)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags {
        read: bool,
        write: bool,
        execute: bool,
        cow: bool,
        private: bool,
        shared: bool,
    }

    impl Flags {
        #[must_use]
        pub fn build() -> FlagBuilder {
            FlagBuilder::new()
        }

        /// Convert a `Flags` into a `FlagBuilder`, whose fields can be modified; it will need to be
        /// re-validated to get back a `Flags` object.
        #[must_use]
        pub const fn into_builder(self) -> FlagBuilder {
            FlagBuilder {
                read: self.read,
                write: self.write,
                execute: self.execute,
                cow: self.cow,
                private: self.private,
                shared: self.shared,
            }
        }

        flag_constructor!(read);
        flag_constructor!(write);
        flag_constructor!(execute);
        flag_constructor!(cow);
        flag_constructor!(private);
        flag_constructor!(shared);
    }

    /// Create a new `Flag`s object.
    ///
    /// ```
    /// # use reedos_address_space::{Flags, flags};
    /// assert_eq!(flags![read, write], Flags::build().toggle_read().toggle_write().validate());
    /// ```
    #[macro_export]
    macro_rules! flags [
    ($($flag:ident),*) => {
        $crate::address_space::FlagBuilder {
            $(
                $flag: true,
            )*
            ..$crate::address_space::FlagBuilder::new()
            }
            .validate()
        };
    ];
}

pub use crate::flags;
pub use flags::{FlagBuilder, Flags};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_source::DsError;
    use parking_lot::RwLock;

    // Use std for testing only.
    extern crate std;
    use std::vec::Vec;

    /// A proxy data soucre for testing.
    #[derive(Debug)]
    struct ProxyDs<const CAPACITY: usize> {
        buffer: RwLock<[u8; CAPACITY]>,
    }

    impl<const CAPACITY: usize> PartialEq for ProxyDs<CAPACITY> {
        fn eq(&self, other: &Self) -> bool {
            // SAFETY: cannot deadlock since we can have multiple readers
            *self.buffer.read() == *other.buffer.read()
        }
    }

    impl<const CAPACITY: usize> Eq for ProxyDs<CAPACITY> {}

    impl<const CAPACITY: usize> ProxyDs<CAPACITY> {
        const fn new() -> Self {
            Self {
                buffer: RwLock::new([0; CAPACITY]),
            }
        }

        pub fn assert_eq(&self, other: [u8; CAPACITY]) {
            assert_eq!(*self.buffer.read(), other);
        }
    }

    impl<const CAPACITY: usize> DataSource for ProxyDs<CAPACITY> {
        fn read(&self, offset: usize, length: usize, buffer: &mut [u8]) -> Result<(), DsError> {
            assert!(offset + length <= CAPACITY);

            buffer.copy_from_slice(&self.buffer.read()[offset..offset + length]);
            Ok(())
        }

        fn write(&self, offset: usize, length: usize, buffer: &[u8]) -> Result<(), DsError> {
            assert!(offset + length <= CAPACITY);

            self.buffer.write()[offset..offset + length].copy_from_slice(buffer);
            Ok(())
        }

        fn flush(&self, offset: usize, length: usize) -> Result<(), DsError> {
            assert!(offset + length <= CAPACITY);

            self.buffer.write()[offset..offset + length].fill(0);
            Ok(())
        }
    }

    #[test]
    fn proxy_ds_works() -> Result<(), DsError> {
        const TEST_DS_CAPACITY: usize = 32;

        let ds = ProxyDs::<32>::new();

        // write all 1s
        ds.write(0, 32, &[1; 32])?;
        ds.assert_eq([1; 32]);

        // read 15 bytes from the middle
        let mut buffer = [0; 15];
        ds.read(10, 15, &mut buffer)?;
        assert_eq!(buffer, [1; 15]);

        // write from 0 to 31
        let mut write_from = [0u8; 32];
        for i in 0..32 {
            write_from[i as usize] = i;
        }
        ds.write(0, 32, &write_from)?;
        ds.assert_eq(write_from);

        // read 12 bytes from the middle
        let mut read_into = [0; 12];
        ds.read(13, 12, &mut read_into)?;
        assert_eq!(read_into, write_from[13..25]);

        Ok(())
    }

    #[test]
    fn constructor() {
        // Construct an address space with capacity 20.
        let space = AddressSpace::<1200>::new("my first address space");
        assert_eq!(space.name, "my first address space");
    }

    fn test_add_mapping_once(length: usize) -> Result<(), AsError> {
        const DS_CAPACITY: usize = 16;
        const N_PAGES: usize = 1200;
        const PAGE_SIZE: usize = 20;

        let mut space = AddressSpace::<N_PAGES, PAGE_SIZE>::new("test space");
        let source = ProxyDs::<DS_CAPACITY>::new();

        let addr = space.add_mapping(&source, length)?;

        std::dbg!(&space);
        space.assert_valid();

        assert_ne!(addr, 0);
        assert!(!space.mappings.is_empty());

        let mapping = space.mappings.first().expect("source was mapped");

        assert_eq!(mapping.addr, addr);
        assert_eq!(mapping.length, length);
        // TODO: check DS equality

        Ok(())
    }

    #[test]
    fn add_mapping_once_works() -> Result<(), AsError> {
        test_add_mapping_once(1)?;
        test_add_mapping_once(200)?;

        Ok(())
    }

    #[test]
    fn add_mapping_works() -> Result<(), AsError> {
        const N_ADDRS: usize = 100;
        const DS_CAPACITY: usize = 16;
        const N_PAGES: usize = 1200;
        const PAGE_SIZE: usize = 20;

        let mut space = AddressSpace::<N_PAGES, PAGE_SIZE>::new("test space");
        let source = ProxyDs::<DS_CAPACITY>::new();

        let mut addrs = Vec::new();

        for l in 1..=N_ADDRS {
            addrs.push(space.add_mapping(&source, l)?);
            std::dbg!(&space);
            space.assert_valid();
        }

        // Assert none are 0.
        assert!(!addrs.iter().any(|&n| n == 0));

        // Assert all are distinct.
        assert!(addrs.len() == N_ADDRS);

        // assert_eq!(mapping.addr, addr);
        // assert_eq!(mapping.length, length);
        // TODO: check DS equality

        Ok(())
    }
}
