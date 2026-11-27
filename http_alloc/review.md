# Review: `http_alloc/src/arena.rs`

Status: **review only — no changes applied.** Written 2026-11-27.
File reviewed: `http_alloc/src/arena.rs` (working tree as of this date).

## Scope & context

`http_alloc` is the workspace's custom-allocation crate. As of this review it is
**not referenced by any other crate** (`http_1`, `http_headers`, `http_url`,
`http_primitives` do not depend on it) — it is exercised only by its own unit and
bolero tests.

The arena is a page-backed bump allocator:

- `region::alloc(size, READ_WRITE)` backs a `region::Allocation` (rounded up to page size).
- Per-allocation layout:
  ```
  [ block (align block to layout.align()) ] [ padding ] [ usize = previous head ]
  ```
  The `usize` word **immediately below `head`** records where `head` was *before* that
  allocation. `head` = end of the most recent metadata word (or the region start).
- The returned slice covers exactly `layout.size()` bytes and is always fully contained
  in the region (bounds-checked at `head_new`).

Mechanisms on top:
- `deallocate` — head-pop optimization: if the block being deallocated is the head block
  (`alloc_start == ptr`), `head` is rewound to the previous head. Non-head deallocations
  are no-ops (conservative leak, correct for a bump arena).
- `grow` — head-block in-place growth (keeps `Vec` in place across capacity increases)
  with a fallback to allocate-new + `ptr::copy_nonoverlapping`.

## Verification performed

- `cargo test -p http_alloc` — **7/7 pass** (`alloc_simple`, in-place grow success/fail,
  grow-after-free, allocate/grow failure paths, `validate::model` bolero model).
- `cargo clippy -p http_alloc --all-features` — 18 warnings, all `missing_docs` on
  `Arena` / `new` / `available` / `capacity` (workspace `missing_docs = "warn"`).
- Runtime probe (standalone crate in `/tmp`, copy of `arena.rs`, x86_64):
  ```
  after alloc(4): available = 4080
  grow(4→8):  ptr == old? true   available = 4060
  grow(8→16): ptr == old? true   available = 4036
  dealloc head block: available = 4096 (pop works)
  ```
  This confirms the in-place grow writes dead/stale bytes below head and shrinks
  `available()` — see finding 1.
- `cargo miri` cannot verify this crate: `region` uses `mmap`, which miri does not
  support. Note the `fix(alloc): miri` commit targets tests whose backing memory is not
  mmap-backed (or is not reachable by miri); the in-crate grow tests do not prove absence
  of UB here.

## Finding 1 (critical): metadata word in in-place grow is written to an unaligned address

**Location:** grow's in-place branch — `http_alloc/src/arena.rs:206-237`
(`let size_align = size_layout.align()` at line 207, word written at line 237).

In `allocate` (line 105) the padding between the block end and the metadata word is
computed correctly with `align_offset(size_layout.align())`, so the word is always
8-aligned. In the in-place grow branch instead:

```rust
let size_align = size_layout.align();          // constant 8, NOT an offset
let size_start = match alloc_stop.checked_add(size_align) { ... };
// ...
unsafe { std::ptr::write(size_start_ptr, head_prev.addr().get()) };  // line 237
```

`alloc_stop` is 8-aligned only when `len_new - len_old` is a multiple of 8. Otherwise
`size_start` is 4-aligned or worse, and `size_stop` (head) is unaligned. Consequences:

1. Line 237 writes a `usize` to a potentially **unaligned** address — immediate UB.
2. The head is left unaligned; every later `head_prev()` (line 57: `head.cast::<usize>().sub(1)`)
   reads `usize` from an unaligned address and `read()`s it — also UB.
3. `available()` is computed from a head that was pushed too far: the probed numbers
   (4080 → 4060 → 4036) show each in-place grow forfeits ~16–36 dead bytes permanently
   (the previous, now-below-head word is never overwritten), so `available()` under-reports
   and near-capacity grows can fail spuriously.

### Why it can't bite today

All unit tests pass on x86_64 because unaligned accesses are silently tolerated there.
The probe also never re-reads the word after a grow through `head_prev()` with the same
`Vec` (the failure only surfaces once the grown block is grown or deallocated with a layout
whose `align_offset` differs, or at other alignment-sensitive points). On strict-alignment
targets the program can trap or misbehave.

### Proposed fix

Rewrite the branch to use the real block geometry (lines 198–245):

```rust
let new_block_end = alloc_start + len_new;
let new_block_end_ptr = ptr.with_addr(new_block_end);          // alloc_start already computed
let new_word_start = new_block_end + new_block_end_ptr.align_offset(align_of::<usize>());
let new_word_end = new_word_start + size_len;
// bounds-check new_word_end against range.end, then:
//   write head_prev at new_word_start
//   head = new_word_end
//   return slice [ptr_prev, len_new)
```

This both fixes the alignment and the head over-extension (the correct base is
`alloc_start + len_new`, not `head + (len_new - len_old)`, which already includes the old
padding + old word).

## Finding 2: allocator methods must not unwind

**Location:** `allocate` (lines 83, 98-99, 118-119, 141-144), `grow` (lines 177, 182, 203-204,
220-221, 239-241), `deallocate` (line 152), `head_prev()` (lines 51, 59, 66-67).

The `Allocator` contract states that none of the allocating/deallocating methods may panic
or unwind. These asserts (`assert!`, `assert_ne!`) can fire under ordinary usage:

- `grow`: `assert!(len_old <= len_new)` — violates contract only on caller error (must not
  be reachable by safe code), but panics in an allocating method are unconditionally
  contract-violating.
- `allocate` / `grow`: internal invariants (`contains`, `head != start`) should be `Err`,
  never panic.

Also note any panic inside `allocate`/`grow` (a `Vec::push` call site) is a potential hard
abort in library code.

## Finding 3: `capacity()` reports the page-rounded size, not the requested size

**Location:** `capacity()` (lines 43-45). `region::alloc` rounds up to the page size, so
`Arena::new(1)` reports `capacity() == 4096`. Same for `available()` — both ultimately
report the rounded-up backing size. Harmless but surprising; document it or store the
requested `size` and report that (or keep the method and let callers reason about it).

## Finding 4: head-pop semantics — documented but unverified in head-of-growing cases

The head-pop in `deallocate` is logically correct (traced manually): it rewinds to the
previous head recorded in the metadata word, which is exactly the start of the popped
region. Non-head blocks are intentionally never reclaimed (bump-arena semantics). Two
caveats worth stating in the file:

- Non-head deallocations are permanent leaks *by design* — undocumented. Don't rely on
  `available()` being tight for un-growable/un-poppable blocks.
- In the fallback `grow` path, the old block is never deallocated (it cannot be — interior
  block). This is correct but invisible; a comment would future-proof the intent.

## Finding 5: documentation/diagnostics gaps

- `Arena`, `new`, `available`, `capacity` have no docs — clippy's `missing_docs` warnings.
- `available()` is misleading once Finding 1 exists (shrinks despite live capacity).
- No doc comment records the two documented panic sites (`head_prev` line 47-49).

## What is verified correct

- Basic `allocate` math (block alignment, padding, 8-aligned word, bounds-checked
  `head_new < range.end`), and the returned slice is always contained in the region.
- `deallocate` head-pop rewound correctly for all the cases I traced (original block,
  in-place-grown block, fallback-grown block, popped-then-reallocated block).
- `available()` never underflows (`head < range.end` asserted).
- Fallback `grow`: `ptr_new` does not overlap the old block (new block starts at/after
  the current head; old block is strictly below it), and `copy_nonoverlapping` with
  `layout_old.size()` is sound. Returned slice is valid and contained.
- `Arena` is intentionally single-threaded (`Cell`); no `Sync` required for its stated use.

## Not verified / left to the reader

- No miri run is possible (mmap backing) — the alignment bug in Finding 1 can only be
  proven with a strict-alignment target or a non-mmap probe.
- The `validate::model` bolero test is a smoke test (1k iterations-ish); it would not fail
  on Finding 1 because it never re-reads a grown head's metadata word.
- `macro_util`/`macro_derive` in the workspace are out of scope here.

## Suggested follow-ups

1. Fix the in-place grow word placement (Finding 1) — highest priority.
2. Replace panicking allocating-method asserts with `Err(AllocError)` (Finding 2).
3. Document `capacity()`/`available()` page-rounding and the head-pop/leak semantics.
4. Add docs on `Arena` + public methods (silences the 18 clippy warnings).
5. Consider a regression test asserting `available()` is tight after each in-place grow,
   and an alignment assertion on the word (`% align_of::<usize>() == 0`).

