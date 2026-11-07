// The crate is `no_std`, but the test harness links `std`, so pull in the items the tests need
// (including the `vec!` and `thread_local!` macros) that the `core` prelude doesn't provide.
#[cfg(feature = "from-iter")]
use std::boxed::Box;
use std::cell::Cell;
use std::string::{String, ToString};
use std::vec::Vec;
use std::{thread_local, vec};

use super::{DroplessArena, TypedArena};

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
struct Point {
    x: i32,
    y: i32,
    z: i32,
}

impl<T> TypedArena<T> {
    /// Clears the arena. Deallocates all but the longest chunk which may be reused.
    fn clear(&mut self) {
        unsafe {
            // Clear the last chunk, which is partially filled.
            let mut chunks_borrow = self.chunks.borrow_mut();
            if let Some(last_chunk) = chunks_borrow.last_mut() {
                self.clear_last_chunk(last_chunk);
                let len = chunks_borrow.len();
                for mut chunk in chunks_borrow.drain(..len - 1) {
                    chunk.destroy(chunk.entries);
                }
            }
        }
    }
}

#[test]
fn test_unused() {
    let arena: TypedArena<Point> = TypedArena::default();
    assert!(arena.chunks.borrow().is_empty());
}

#[test]
fn test_unused_dropless() {
    let arena = DroplessArena::default();
    assert!(arena.chunks.borrow().is_empty());
}

#[test]
fn test_arena_alloc_nested() {
    struct Inner {
        value: u8,
    }
    struct Outer<'a> {
        inner: &'a Inner,
    }
    enum EI<'e> {
        I(Inner),
        O(Outer<'e>),
    }

    struct Wrap(DroplessArena);

    impl Wrap {
        fn alloc_inner<F: Fn() -> Inner>(&self, f: F) -> &Inner {
            match self.0.alloc(EI::I(f())) {
                EI::I(i) => i,
                _ => panic!("mismatch"),
            }
        }
        fn alloc_outer<'a, F: Fn() -> Outer<'a>>(&'a self, f: F) -> &'a Outer<'a> {
            match self.0.alloc(EI::O(f())) {
                EI::O(o) => o,
                _ => panic!("mismatch"),
            }
        }
    }

    let arena = Wrap(DroplessArena::default());

    let result = arena.alloc_outer(|| Outer {
        inner: arena.alloc_inner(|| Inner { value: 10 }),
    });

    assert_eq!(result.inner.value, 10);
}

#[test]
fn test_copy() {
    let arena = TypedArena::default();
    #[cfg(not(miri))]
    const N: usize = 100000;
    #[cfg(miri)]
    const N: usize = 1000;
    for _ in 0..N {
        arena.alloc(Point { x: 1, y: 2, z: 3 });
    }
}

// #[bench]
// fn bench_copy(b: &mut Bencher) {
//     let arena = TypedArena::default();
//     b.iter(|| arena.alloc(Point { x: 1, y: 2, z: 3 }))
// }
//
// #[bench]
// fn bench_copy_nonarena(b: &mut Bencher) {
//     b.iter(|| {
//         let _: Box<_> = Box::new(Point { x: 1, y: 2, z: 3 });
//     })
// }

#[allow(dead_code)]
struct Noncopy {
    string: String,
    array: Vec<i32>,
}

#[test]
fn test_noncopy() {
    let arena = TypedArena::default();
    #[cfg(not(miri))]
    const N: usize = 100000;
    #[cfg(miri)]
    const N: usize = 1000;
    for _ in 0..N {
        arena.alloc(Noncopy {
            string: "hello world".to_string(),
            array: vec![1, 2, 3, 4, 5],
        });
    }
}

#[test]
fn test_typed_arena_clear() {
    let mut arena = TypedArena::default();
    for _ in 0..10 {
        arena.clear();
        #[cfg(not(miri))]
        const N: usize = 10000;
        #[cfg(miri)]
        const N: usize = 100;
        for _ in 0..N {
            arena.alloc(Point { x: 1, y: 2, z: 3 });
        }
    }
}

// #[bench]
// fn bench_typed_arena_clear(b: &mut Bencher) {
//     let mut arena = TypedArena::default();
//     b.iter(|| {
//         arena.alloc(Point { x: 1, y: 2, z: 3 });
//         arena.clear();
//     })
// }
//
// #[bench]
// fn bench_typed_arena_clear_100(b: &mut Bencher) {
//     let mut arena = TypedArena::default();
//     b.iter(|| {
//         for _ in 0..100 {
//             arena.alloc(Point { x: 1, y: 2, z: 3 });
//         }
//         arena.clear();
//     })
// }

// Drop tests

struct DropCounter<'a> {
    count: &'a Cell<u32>,
}

impl Drop for DropCounter<'_> {
    fn drop(&mut self) {
        self.count.set(self.count.get() + 1);
    }
}

#[test]
fn test_typed_arena_drop_count() {
    let counter = Cell::new(0);
    {
        let arena: TypedArena<DropCounter<'_>> = TypedArena::default();
        for _ in 0..100 {
            // Allocate something with drop glue to make sure it doesn't leak.
            arena.alloc(DropCounter { count: &counter });
        }
    };
    assert_eq!(counter.get(), 100);
}

#[test]
fn test_typed_arena_drop_on_clear() {
    let counter = Cell::new(0);
    let mut arena: TypedArena<DropCounter<'_>> = TypedArena::default();
    for i in 0..10 {
        for _ in 0..100 {
            // Allocate something with drop glue to make sure it doesn't leak.
            arena.alloc(DropCounter { count: &counter });
        }
        arena.clear();
        assert_eq!(counter.get(), i * 100 + 100);
    }
}

thread_local! {
    static DROP_COUNTER: Cell<u32> = Cell::new(0)
}

#[allow(unused)]
struct SmallDroppable(u8);

impl Drop for SmallDroppable {
    fn drop(&mut self) {
        DROP_COUNTER.with(|c| c.set(c.get() + 1));
    }
}

#[test]
fn test_typed_arena_drop_small_count() {
    DROP_COUNTER.with(|c| c.set(0));
    {
        let arena: TypedArena<SmallDroppable> = TypedArena::default();
        for _ in 0..100 {
            // Allocate something with drop glue to make sure it doesn't leak.
            arena.alloc(SmallDroppable(0));
        }
        // dropping
    };
    assert_eq!(DROP_COUNTER.with(|c| c.get()), 100);
}

// #[bench]
// fn bench_noncopy(b: &mut Bencher) {
//     let arena = TypedArena::default();
//     b.iter(|| {
//         arena.alloc(Noncopy { string: "hello world".to_string(), array: vec![1, 2, 3, 4, 5] })
//     })
// }
//
// #[bench]
// fn bench_noncopy_nonarena(b: &mut Bencher) {
//     b.iter(|| {
//         let _: Box<_> =
//             Box::new(Noncopy { string: "hello world".to_string(), array: vec![1, 2, 3, 4, 5] });
//     })
// }

struct WithInternalRef<'a> {
    number: u32,
    next: Option<&'a WithInternalRef<'a>>,
}

#[test]
fn test_dropless_with_reference() {
    let arena = DroplessArena::default();
    let second = arena.alloc(WithInternalRef {
        number: 2,
        next: None,
    });
    let first = arena.alloc(WithInternalRef {
        number: 1,
        next: Some(second),
    });
    assert_eq!(first.number, 1);
    assert_eq!(first.next.unwrap().number, 2);
}

#[test]
fn test_dropless_slice() {
    let arena = DroplessArena::default();
    let slice = arena.alloc_slice(&[1, 2, 3, 4, 5]);
    assert_eq!(slice, &[1, 2, 3, 4, 5]);
}

#[test]
fn test_dropless_str() {
    let arena = DroplessArena::default();
    let string = arena.alloc_str("hello world");
    assert_eq!(string, "hello world");
}

#[cfg(feature = "from-iter")]
#[derive(Debug, PartialEq, Eq)]
struct NotCopyNotDrop {
    value: i32,
}

#[cfg(feature = "from-iter")]
#[test]
fn test_declare_arena() {
    crate::declare_arena!([
        ints: NotCopyNotDrop,
        boxes: Box<i32>,
    ]);

    let arena = Arena::default();

    // `Copy` types that you want to allocate don't need to be mentioned in `declare_arena!`.
    let num = arena.alloc(1);
    assert_eq!(num, &1);

    let slice = arena.alloc_slice(&[10, 15, 17]);
    assert_eq!(slice, &[10, 15, 17]);

    let val = arena.alloc(NotCopyNotDrop { value: 2 });
    assert_eq!(val.value, 2);

    let slice = arena.alloc_from_iter((1..3).into_iter().map(|n| NotCopyNotDrop { value: n }));
    assert_eq!(
        slice,
        &[NotCopyNotDrop { value: 1 }, NotCopyNotDrop { value: 2 }]
    );

    let boxed = arena.alloc(Box::new(2));
    assert_eq!(boxed, &Box::new(2));

    let string = arena.alloc_str("hello world");
    assert_eq!(string, "hello world");
}

struct CycleParticipant<'a> {
    other: Cell<Option<&'a CycleParticipant<'a>>>,
}

#[test]
fn test_cycle() {
    let arena = DroplessArena::default();

    let a = arena.alloc(CycleParticipant {
        other: Cell::new(None),
    });
    let b = arena.alloc(CycleParticipant {
        other: Cell::new(None),
    });

    a.other.set(Some(b));
    b.other.set(Some(a));
}
