# stable-arena

This is the arena from rustc, from [this commit](https://github.com/rust-lang/rust/blob/1c4af67116792ba7674d4c3a488137a8d0ff6520/compiler/rustc_arena/src/lib.rs),
modified minimally in order to be usable on stable Rust.
All credit goes to the Rust Project Developers.

The crate defines two arena types: `TypedArena` and `DroplessArena`, one macro: `declare_arena!`, and two marker types: `IsCopy` and `IsNotCopy`.
See the documentation on how to use them.

One of the modifications that was necessary to make it work on stable Rust is to remove the `#[may_dangle]` attribute from the `Drop` implementation of `TypedArena`,
which has some unfortunate consequences, as described below.

## Modifications from the original
### `#[may_dangle]`
In the original, the `Drop` implementation for `TypedArena` is defined like this:

```rust
unsafe impl<#[may_dangle] T> Drop for TypedArena<T> {
    fn drop(&mut self) {
        // ...drop code...
    }
}
```

However, `#[may_dangle]` is not stable yet (and may never be): https://github.com/rust-lang/rust/issues/34761.
So, it cannot be used in this crate.

This means that structs with internal references to the arena cannot be used within the `TypedArena` from this crate,
because the borrow checker will complain that the arena doesn’t live long enough.
Such an internal reference could for example look like this:

```rust
struct LinkedList<'a> {
    value: u32,
    next: Option<&'a LinkedList<'a>>,
}

fn main() {
    let arena: TypedArena<LinkedList<'_>> = TypedArena::default();
    let second = arena.alloc(LinkedList {
        value: 2,
        next: None,
    });
    let first = arena.alloc(LinkedList {
        value: 1,
        next: Some(second), // Stores a reference to `second`.
    });
    assert_eq!(first.value, 1);
    assert_eq!(first.next.unwrap().value, 2);
}
```

This compiles with rustc’s `TypedArena` but not with this crate’s `TypedArena`.
However, you can use `DroplessArena` instead (`DroplessArena` even allows reference *cycles*; see the documentation).
The downside is, of course, that your allocated objects won’t be *dropped* then.
This means that types with internal references *and* which need to be dropped cannot be stored in any arena in this crate.

### `intrinsics::assume`
The original has this code:

```rust
// Tell LLVM that `end` is aligned to DROPLESS_ALIGNMENT.
unsafe { intrinsics::assume(end == align_down(end, DROPLESS_ALIGNMENT)) };
```

This was replaced by `std::hint::assume_unchecked`, which is the stable equivalent since Rust 1.81.0.

### `no_std` support
The original is part of the compiler and freely uses `std`. This crate is `#![no_std]` instead: the
`std::` imports were changed to `core::` and `alloc::` (and the macro emits `::core::` paths), with
`Box`/`Vec` coming from `alloc`. This requires a global allocator but no other part of `std`.

### The never type `!` in `try_alloc_from_iter`
The original expresses the infallible `alloc_from_iter` in terms of the fallible `try_alloc_from_iter`
by using the never type `!`:

```rust
pub fn alloc_from_iter<I: IntoIterator<Item = T>>(&self, iter: I) -> &mut [T] {
    self.try_alloc_from_iter(iter.into_iter().map(Ok::<T, !>)).into_ok()
}
```

The never type `!` and `Result::into_ok` (the `unwrap_infallible` feature) are both unstable, so this
crate uses the stable [`std::convert::Infallible`](https://doc.rust-lang.org/std/convert/enum.Infallible.html)
in place of `!` and matches on the (uninhabited) error instead of calling `into_ok`:

```rust
match self.try_alloc_from_iter(iter.into_iter().map(Ok::<T, Infallible>)) {
    Ok(slice) => slice,
    Err(never) => match never {},
}
```

Both `try_alloc_from_iter` and `alloc_from_iter` are gated behind the `from-iter` feature.

### Simplifications in the macro
The macro `declare_arena!` was originally written primarily for the purpose where structs have internal references to the arena.
For this purpose, it had a hard-coded lifetime parameter `'tcx`, which referred to the lifetime of the arena.
This lifetime parameter was removed, because the intended use case is not supported by this crate anyway.

Another simplification for the macro was to remove an optional first parameter for the sub-arenas.
This parameter was not used in the macro itself.
I’m not sure whether it still has a purpose in the original context of the rust compiler, but I couldn’t divine one.
