#[macro_use]
extern crate once_cell;
extern crate crossbeam_utils;

use crossbeam_utils::thread::scope;

use std::{
    mem,
    thread,
    ptr,
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};
use once_cell::{sync, unsync};

fn go<F: FnOnce() -> ()>(mut f: F) {
    struct Yolo<T>(T);
    unsafe impl<T> Send for Yolo<T> {}

    let ptr: *const u8 = &mut f as *const F as *const u8;
    mem::forget(f);
    let yolo = Yolo(ptr);
    thread::spawn(move || {
        let f: F = unsafe { ptr::read(yolo.0 as *const F) };
        f();
    }).join().unwrap();
}

#[test]
fn unsync_once_cell() {
    let c = unsync::OnceCell::new();
    assert!(c.get().is_none());
    c.get_or_init(|| 92);
    assert_eq!(c.get(), Some(&92));

    c.get_or_init(|| {
        panic!("Kabom!")
    });
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn sync_once_cell() {
    let c = sync::OnceCell::new();
    assert!(c.get().is_none());
    go(|| {
        c.get_or_init(|| 92);
        assert_eq!(c.get(), Some(&92));
    });
    c.get_or_init(|| panic!("Kabom!"));
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn unsync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = unsync::OnceCell::new();
    x.get_or_init(|| Dropper);
    assert_eq!(DROP_CNT.load(SeqCst), 0);
    drop(x);
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn sync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = sync::OnceCell::new();
    go(|| {
        x.get_or_init(|| Dropper);
        assert_eq!(DROP_CNT.load(SeqCst), 0);
        drop(x);
    });
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn unsync_once_cell_drop_empty() {
    let x = unsync::OnceCell::<String>::new();
    drop(x);
}

#[test]
fn sync_once_cell_drop_empty() {
    let x = sync::OnceCell::<String>::new();
    drop(x);
}

#[test]
fn unsync_lazy_macro() {
    let called = Cell::new(0);
    let x = unsync_lazy! {
        called.set(called.get() + 1);
        92
    };

    assert_eq!(called.get(), 0);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);
}

#[test]
fn sync_lazy_macro() {
    let called = AtomicUsize::new(0);
    let x = sync_lazy! {
        called.fetch_add(1, SeqCst);
        92
    };

    assert_eq!(called.load(SeqCst), 0);

    go(|| {
        let y = *x - 30;
        assert_eq!(y, 62);
        assert_eq!(called.load(SeqCst), 1);
    });

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.load(SeqCst), 1);
}


#[test]
fn static_lazy() {
    static XS: sync::Lazy<Vec<i32>> = sync_lazy! {
        let mut xs = Vec::new();
        xs.push(1);
        xs.push(2);
        xs.push(3);
        xs
    };
    go(|| {
        assert_eq!(&*XS, &vec![1, 2, 3]);
    });
    assert_eq!(&*XS, &vec![1, 2, 3]);
}

#[test]
fn static_lazy_no_macros() {
    fn xs() -> &'static Vec<i32> {
        static XS: sync::OnceCell<Vec<i32>> = sync::OnceCell::INIT;
        XS.get_or_init(|| {
            let mut xs = Vec::new();
            xs.push(1);
            xs.push(2);
            xs.push(3);
            xs
        })
    }
    assert_eq!(xs(), &vec![1, 2, 3]);
}

#[test]
fn sync_once_cell_is_sync_send() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<sync::OnceCell<String>>();
}

#[test]
fn eval_once_macro() {
    macro_rules! eval_once {
        (|| -> $ty:ty {
            $($body:tt)*
        }) => {{
            static ONCE_CELL: sync::OnceCell<$ty> = sync::OnceCell::INIT;
            fn init() -> $ty {
                $($body)*
            }
            ONCE_CELL.get_or_init(init)
        }};
    }

    let fib: &'static Vec<i32> = eval_once! {
        || -> Vec<i32> {
            let mut res = vec![1, 1];
            for i in 0..10 {
                let next = res[i] + res[i + 1];
                res.push(next);
            }
            res
        }
    };
    assert_eq!(fib[5], 8)
}

#[test]
fn sync_once_cell_does_not_leak_partially_constructed_boxes() {
    let n_tries = 100;
    let n_readers = 10;
    let n_writers = 3;
    const MSG: &str = "Hello, World";

    for _ in 0..n_tries {
        let cell: sync::OnceCell<String> = sync::OnceCell::INIT;
        scope(|scope| {
            for _ in 0..n_readers {
                scope.spawn(|| loop {
                    if let Some(msg) = cell.get() {
                        assert_eq!(msg, MSG);
                        break;
                    }
                });
            }
            for _ in 0..n_writers {
                scope.spawn(|| cell.set(MSG.to_owned()));
            }
        })
    }
}
