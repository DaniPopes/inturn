use crate::{InternerSymbol, Symbol};
use boxcar::Vec as LFVec;
use bumpalo::Bump;
use std::{
    collections::hash_map::RandomState, hash::BuildHasher, marker::PhantomData, ptr, slice,
    sync::PoisonError,
};
use thread_local::ThreadLocal;

#[cfg(loom)]
use loom::sync::{
    Mutex,
    atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};
#[cfg(not(loom))]
use std::sync::{
    Mutex,
    atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};

/// Number of shards. Only insertions of new strings contend on a shard.
///
/// Loom explores every interleaving, so it gets two shards and tables that grow on the second
/// insertion.
const SHARDS: usize = if cfg!(loom) { 2 } else { 64 };
const SHARD_SHIFT: u32 = u64::BITS - SHARDS.ilog2();
/// Odd multiplier that spreads all hash bits into the top bits used to select a shard.
const SHARD_MIX: u64 = 0x9E37_79B9_7F4A_7C15;
/// Smallest number of slots in a shard's table. Must be a power of two.
const MIN_SLOTS: usize = if cfg!(loom) { 2 } else { 32 };

// TODO: Use a lock-free arena.
type Arenas = ThreadLocal<Bump>;

/// Byte string interner.
///
/// See the [crate-level docs][crate] for more details.
///
/// Holds at most `u32::MAX - 1` strings; interning more panics.
pub struct BytesInterner<S = Symbol, H = RandomState> {
    shards: Box<[Shard; SHARDS]>,
    hash_builder: H,
    strs: LFVec<&'static [u8]>,
    arena: Arenas,
    _symbol: PhantomData<S>,
}

impl_interner_api!(
    BytesInterner,
    value = [u8],
    mut_note = "this never acquires any locks.",
    cheap_op = "cheap, lock-free operation"
);

impl<S: InternerSymbol, H: BuildHasher> BytesInterner<S, H> {
    fn with_capacity_and_hasher_impl(capacity: usize, hash_builder: H) -> Self {
        // Keep the load factor at or below 50%.
        let slots = (capacity / SHARDS * 2).next_power_of_two().max(MIN_SLOTS);
        let Ok(shards) = (0..SHARDS).map(|_| Shard::new(slots)).collect::<Box<[_]>>().try_into()
        else {
            unreachable!()
        };
        let strs = LFVec::with_capacity(capacity);
        Self { shards, strs, arena: Default::default(), hash_builder, _symbol: PhantomData }
    }

    #[inline]
    fn len_impl(&self) -> usize {
        self.strs.count()
    }

    #[inline]
    fn intern_impl(&self, s: &[u8]) -> S {
        self.do_intern(s, alloc)
    }

    #[inline]
    fn intern_mut_impl(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, alloc)
    }

    #[inline]
    fn intern_static_impl(&self, s: &'static [u8]) -> S {
        self.do_intern(s, no_alloc)
    }

    #[inline]
    unsafe fn intern_static_unchecked_impl(&self, s: &[u8]) -> S {
        self.do_intern(s, no_alloc_unchecked)
    }

    #[inline]
    fn intern_mut_static_impl(&mut self, s: &'static [u8]) -> S {
        self.do_intern_mut(s, no_alloc)
    }

    #[inline]
    unsafe fn intern_mut_static_unchecked_impl(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, no_alloc_unchecked)
    }

    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    fn resolve_impl(&self, sym: S) -> &[u8] {
        if cfg!(debug_assertions) {
            self.strs.get(sym.to_usize()).expect("symbol out of bounds")
        } else {
            unsafe { self.strs.get_unchecked(sym.to_usize()) }
        }
    }

    #[inline]
    fn try_resolve_impl(&self, sym: S) -> Option<&[u8]> {
        self.strs.get(sym.to_usize()).copied()
    }

    #[inline]
    fn get_impl(&self, s: &[u8]) -> Option<S> {
        let hash = self.hash(s);
        self.shards[shard_index(hash)].find(hash as u32, s).map(S::from_usize)
    }

    #[inline]
    fn do_intern<'a>(
        &self,
        s: &'a [u8],
        alloc: impl FnOnce(&Arenas, &'a [u8]) -> &'static [u8],
    ) -> S {
        let hash = self.hash(s);
        let shard = &self.shards[shard_index(hash)];
        let tag = hash as u32;

        if let Some(i) = shard.find(tag, s) {
            return S::from_usize(i);
        }

        let mut state = shard.state.lock().unwrap_or_else(PoisonError::into_inner);
        get_or_insert(&self.strs, &self.arena, s, tag, &shard.cur, &mut state, alloc)
    }

    #[inline]
    fn do_intern_mut<'a>(
        &mut self,
        s: &'a [u8],
        alloc: impl FnOnce(&Arenas, &'a [u8]) -> &'static [u8],
    ) -> S {
        let hash = self.hash(s);
        let tag = hash as u32;
        let Shard { cur, state } = &mut self.shards[shard_index(hash)];
        let state = state.get_mut().unwrap_or_else(PoisonError::into_inner);
        get_or_insert(&self.strs, &self.arena, s, tag, cur, state, alloc)
    }

    #[inline]
    fn hash(&self, s: &[u8]) -> u64 {
        // We don't use `self.hash_builder.hash_one(s)` because we want to avoid hashing the length.
        use std::hash::Hasher;
        let mut h = self.hash_builder.build_hasher();
        h.write(s);
        h.finish()
    }
}

#[inline]
fn get_or_insert<'a, S: InternerSymbol>(
    strs: &LFVec<&'static [u8]>,
    arena: &Arenas,
    s: &'a [u8],
    tag: u32,
    cur: &AtomicPtr<Table>,
    state: &mut ShardState,
    alloc: impl FnOnce(&Arenas, &'a [u8]) -> &'static [u8],
) -> S {
    // Another thread may have inserted `s` since the lock-free lookup.
    if let Some(i) = load_table(cur).find(tag, s) {
        return S::from_usize(i);
    }
    let s = alloc(arena, s);
    let i = strs.push(s);
    let new_sym = S::from_usize(i);
    state.insert(cur, tag, s, i);
    new_sym
}

#[inline]
fn load_table(cur: &AtomicPtr<Table>) -> &Table {
    // SAFETY: `cur` points into a `Box` in the `ShardState::tables` of its shard. Tables are only
    // appended and never mutated through a unique reference until the shard is dropped, which
    // needs the borrow of `cur` to have ended.
    unsafe { &*cur.load(Ordering::Acquire) }
}

#[inline]
fn alloc(arena: &Arenas, s: &[u8]) -> &'static [u8] {
    // SAFETY: Extends the lifetime of `&Bump` to `'static`. This is never exposed so it's ok.
    unsafe {
        std::mem::transmute::<&[u8], &'static [u8]>(arena.get_or_default().alloc_slice_copy(s))
    }
}

#[inline]
fn no_alloc(_: &Arenas, s: &'static [u8]) -> &'static [u8] {
    s
}

#[inline]
fn no_alloc_unchecked(_: &Arenas, s: &[u8]) -> &'static [u8] {
    // SAFETY: Callers guarantee that `s` remains valid and immutable until the interner is dropped.
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(s) }
}

#[inline]
fn shard_index(hash: u64) -> usize {
    (hash.wrapping_mul(SHARD_MIX) >> SHARD_SHIFT) as usize
}

/// A shard of the `[u8] -> Symbol` map: an insert-only open-addressing hash table.
///
/// Lookups only load from the table, so hits never write to shared memory. Insertions are
/// serialized by `state`, and grow the table by publishing a new one.
///
/// The shard is selected by the top bits of the hash times an odd constant, so every hash bit
/// counts, even if the hasher only fills the low 32 bits. The low 32 bits of the hash are the `tag`
/// stored in the slot, and the tag's low bits select the first probe. Storing the tag means that
/// growing never rehashes a string.
///
/// Each shard has its own pair of cache lines, so an insertion does not invalidate the lines that
/// lock-free hits on other shards read. Pairs, because some CPUs prefetch the neighbour of a
/// 64-byte line and some have 128-byte lines.
#[repr(align(128))]
struct Shard {
    /// The latest table in `state.tables`.
    ///
    /// Published with `Release` after it is fully built; read with `Acquire`.
    cur: AtomicPtr<Table>,
    state: Mutex<ShardState>,
}

struct ShardState {
    /// Number of strings in the shard.
    len: usize,
    /// Every table ever published, oldest first. Retired tables are only freed when the interner
    /// is dropped, so a reader that loaded an old `cur` can keep probing it. They hold at most as
    /// much memory as the latest table.
    // Boxed so that pointers to the tables stay valid when the vector grows.
    #[expect(clippy::vec_box)]
    tables: Vec<Box<Table>>,
}

/// Linear-probing table with a power-of-two number of slots and a load factor of at most 50%, so
/// every probe sequence ends at an empty slot.
struct Table {
    slots: Box<[Slot]>,
}

/// A slot is written once, under the shard lock, and never changes afterwards.
#[derive(Default)]
struct Slot {
    /// Symbol index plus one; zero means the slot is empty. Written last, with `Release`, so a
    /// reader that loads a nonzero value with `Acquire` also sees the other fields.
    sym: AtomicU32,
    tag: AtomicU32,
    /// The pointer and length of a `&'static [u8]` key.
    ptr: AtomicPtr<u8>,
    len: AtomicUsize,
}

impl Slot {
    /// Returns the symbol index plus one, or zero if the slot is empty.
    ///
    /// The `Acquire` load makes the other fields visible when it returns nonzero.
    #[inline]
    fn sym(&self) -> u32 {
        self.sym.load(Ordering::Acquire)
    }

    #[inline]
    fn tag(&self) -> u32 {
        self.tag.load(Ordering::Relaxed)
    }

    /// Returns the key.
    ///
    /// # Safety
    ///
    /// A call to [`Self::sym`] on this slot must have returned nonzero.
    #[inline]
    unsafe fn key(&self) -> &'static [u8] {
        let (ptr, len) = (self.ptr.load(Ordering::Relaxed), self.len.load(Ordering::Relaxed));
        debug_assert!(!ptr.is_null(), "slot published before its key");
        // SAFETY: `store` wrote a `&'static [u8]`, and the caller's `Acquire` load of `sym` made it
        // visible.
        unsafe { slice::from_raw_parts(ptr, len) }
    }

    /// Fills an empty slot. Must only be called with the shard lock held.
    fn store(&self, sym: u32, tag: u32, key: &'static [u8]) {
        self.tag.store(tag, Ordering::Relaxed);
        self.ptr.store(key.as_ptr().cast_mut(), Ordering::Relaxed);
        self.len.store(key.len(), Ordering::Relaxed);
        self.sym.store(sym, Ordering::Release);
    }
}

impl Shard {
    fn new(slots: usize) -> Self {
        let mut state = ShardState { len: 0, tables: Vec::new() };
        let cur = AtomicPtr::new(state.push(Table::new(slots)));
        Self { cur, state: Mutex::new(state) }
    }

    /// Returns the symbol index of `key`, without locking.
    #[inline]
    fn find(&self, tag: u32, key: &[u8]) -> Option<usize> {
        load_table(&self.cur).find(tag, key)
    }
}

impl ShardState {
    /// Inserts `key`, which must not be in the shard, with symbol index `sym`, growing the table
    /// published in `cur` if needed.
    fn insert(&mut self, cur: &AtomicPtr<Table>, tag: u32, key: &'static [u8], sym: usize) {
        let sym = u32::try_from(sym + 1).expect("too many interned strings");
        let table = self.tables.last().unwrap();
        if (self.len + 1) * 2 > table.slots.len() {
            let grown = table.grown();
            cur.store(self.push(grown), Ordering::Release);
        }
        self.tables.last().unwrap().insert(tag, key, sym);
        self.len += 1;
    }

    /// Keeps `table` alive until the shard is dropped and returns a pointer to it.
    fn push(&mut self, table: Table) -> *mut Table {
        self.tables.push(Box::new(table));
        ptr::from_ref(&**self.tables.last().unwrap()).cast_mut()
    }
}

impl Table {
    fn new(slots: usize) -> Self {
        Self { slots: (0..slots).map(|_| Slot::default()).collect() }
    }

    /// Returns a table with twice the slots and the same entries.
    fn grown(&self) -> Self {
        let new = Self::new(self.slots.len() * 2);
        for slot in &self.slots {
            let sym = slot.sym();
            if sym != 0 {
                // SAFETY: `sym` is nonzero.
                new.insert(slot.tag(), unsafe { slot.key() }, sym);
            }
        }
        new
    }

    #[inline]
    fn find(&self, tag: u32, key: &[u8]) -> Option<usize> {
        let mask = self.mask();
        let mut i = tag as usize & mask;
        loop {
            let slot = &self.slots[i];
            let sym = slot.sym();
            if sym == 0 {
                return None;
            }
            // The key is only loaded when the tag matches.
            // SAFETY: `sym` is nonzero.
            if slot.tag() == tag && unsafe { slot.key() } == key {
                return Some(sym as usize - 1);
            }
            i = (i + 1) & mask;
        }
    }

    /// Fills the first empty slot of the probe sequence. The caller holds the shard lock and
    /// guarantees that there is one.
    fn insert(&self, tag: u32, key: &'static [u8], sym: u32) {
        let mask = self.mask();
        let mut i = tag as usize & mask;
        loop {
            let slot = &self.slots[i];
            if slot.sym.load(Ordering::Relaxed) == 0 {
                slot.store(sym, tag, key);
                return;
            }
            i = (i + 1) & mask;
        }
    }

    /// Returns the mask of slot indices.
    #[inline]
    fn mask(&self) -> usize {
        // SAFETY: Tables are built by `Table::new` with a nonzero number of slots, and `grown`
        // doubles it. Telling the compiler lets it drop the bounds check on every slot access.
        unsafe { std::hint::assert_unchecked(!self.slots.is_empty()) };
        self.slots.len() - 1
    }
}
