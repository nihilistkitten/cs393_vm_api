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
        // Only compares addresses so we can use the `remove` API of `SgSet`.
        self.addr == other.addr
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

    /// Create an iterator over the bounds of free regions.
    fn free_regions(&'a self) -> impl Iterator<Item = (VirtualAddress, VirtualAddress)> + 'a {
        let starts = core::iter::once(0).chain(self.mappings.iter().map(MapEntry::end));
        let ends = self
            .mappings
            .iter()
            .map(|m| m.addr)
            .chain(core::iter::once(Self::total_capacity()));

        starts.zip(ends)
    }

    /// Check if there is space for a mapping of length at addr.
    fn is_space_at(&self, addr: VirtualAddress, length: usize) -> bool {
        // Get iterators over all the starts and ends of free blocks.
        self.free_regions()
            // Find the first free region which ends after addr.
            .find(|(_, e)| *e > addr)
            // Check whether it has room.
            .map_or(false, |(s, e)| {
                s + MIN_GAP_SIZE <= addr && addr + length + MIN_GAP_SIZE < e
            })
    }

    /// Find the space for a page of the given length.
    fn find_space_for(&self, length: usize) -> Option<VirtualAddress> {
        // TODO: perf
        //
        // Get iterators over all the starts and ends of free blocks.
        // Find the first one with enough space.
        self.free_regions().find_map(|(s, e)| {
            // The smallest starting address in this range.
            let start = (s + MIN_GAP_SIZE).next_multiple_of(PAGE_SIZE);
            let end = e - MIN_GAP_SIZE;
            if start > end || end - start < length {
                // not enough space
                None
            } else {
                Some(start)
            }
        })
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

        // There is at least `MIN_GAP_SIZE` space between each mapping.
        for (m1, m2) in iter_1.zip(iter_2) {
            // mappings.iter is in-order, so here we're guaranteed:
            // m1.addr <= m2.addr
            // there is no m3 s.t. m1.addr < m3.addr < m2.addr
            assert!(m1.end() + MIN_GAP_SIZE <= m2.addr);
        }

        // All mappings are `PAGE_SIZE`-aligned.
        for m in self.mappings.iter() {
            assert!(m.addr % PAGE_SIZE == 0);
        }
    }

    /// Add a mapping from a `DataSource` into this `AddressSpace`.
    ///
    /// # Errors
    /// If the desired mapping is invalid.
    pub fn add_mapping<D: DataSource>(
        &mut self,
        source: &'a D,
        length: usize,
    ) -> Result<VirtualAddress, AsError> {
        let addr = self.find_space_for(length).ok_or("no space available")?;
        debug_assert!(self.mappings.insert(MapEntry {
            addr,
            length,
            source: Some(source),
        }));
        Ok(addr)
    }

    /// Add a mapping from `DataSource` into this `AddressSpace` starting at a specific address.
    ///
    /// # Errors
    /// If there is insufficient room subsequent to `start`.
    pub fn add_mapping_at<D: DataSource>(
        &mut self,
        addr: VirtualAddress,
        source: &'a D,
        length: usize,
    ) -> Result<(), AsError> {
        if !self.is_space_at(addr, length) {
            return Err("no space available there");
        }
        debug_assert!(self.mappings.insert(MapEntry {
            addr,
            length,
            source: Some(source),
        }));

        Ok(())
    }

    /// Remove the mapping to `DataSource` that starts at the given address.
    ///
    /// # Errors
    /// If the mapping could not be removed.
    pub fn remove_mapping(&mut self, start: VirtualAddress) -> Result<(), AsError> {
        if !self.mappings.remove(&MapEntry {
            addr: start,
            length: PAGE_SIZE,
            source: None,
        }) {
            return Err("no mapping at that address to remove");
        }

        Ok(())
    }

    /// Look up the `DataSource` and offset within that `DataSource` for a
    /// `VirtualAddress` / `AccessType` in this `AddressSpace`
    ///
    /// # Errors
    /// If this `VirtualAddress` does not have a valid mapping in &self,
    /// or if this `AccessType` is not permitted by the mapping
    #[must_use]
    pub fn get_source_for_addr<D: DataSource>(
        &self,
        addr: VirtualAddress,
        access_type: Flags,
    ) -> Option<&dyn DataSource> {
        self.mappings
            .get(&MapEntry {
                addr,
                length: PAGE_SIZE,
                source: None,
            })
            .and_then(|m| m.source)
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
    ///     .toggle_write()
    ///     .validate();
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
        #[must_use]
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
        #[must_use]
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

    #[test]
    fn add_mapping_at_works() -> Result<(), AsError> {
        let mut space = AddressSpace::<6, 20>::new("test space");
        let source = ProxyDs::<16>::new();

        space.mappings.insert(MapEntry {
            addr: 20,
            length: 20,
            source: Some(&source),
        });

        let addr = 60;
        let length = 20;

        space.add_mapping_at(addr, &source, length)?;
        let mapping = space.mappings.iter().nth(1).expect("second mapping exists");

        assert_eq!(mapping.addr, addr);
        assert_eq!(mapping.length, length);
        space.assert_valid();

        Ok(())
    }

    #[test]
    fn add_mapping_at_err_works() {
        let mut space = AddressSpace::<10, 20>::new("test space");
        let source = ProxyDs::<16>::new();

        space.mappings.insert(MapEntry {
            addr: 20,
            length: 20,
            source: Some(&source),
        });

        assert!(space.add_mapping_at(20, &source, 20).is_err());
        space.assert_valid();
    }

    #[test]
    fn remove_mapping_works() -> Result<(), AsError> {
        let mut space = AddressSpace::<10, 20>::new("test space");
        let source = ProxyDs::<16>::new();

        space.mappings.insert(MapEntry {
            addr: 20,
            length: 20,
            source: Some(&source),
        });

        space.mappings.insert(MapEntry {
            addr: 60,
            length: 20,
            source: Some(&source),
        });

        space.mappings.insert(MapEntry {
            addr: 100,
            length: 20,
            source: Some(&source),
        });

        space.remove_mapping(60)?;

        assert_eq!(space.mappings.len(), 2);

        assert_eq!(space.mappings.first().expect("first exists").addr, 20);
        assert_eq!(space.mappings.last().expect("last exists").addr, 100);

        Ok(())
    }
}
