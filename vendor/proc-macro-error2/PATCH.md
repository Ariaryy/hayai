# Local compatibility patch

Source: proc-macro-error2 2.0.1 from crates.io, checksum
`11ec05c52be0a07b08061f7dd003e7d7092e0472bc731b4af7bb1ef876109802`.
Original MIT and Apache licenses are included.

The only source change makes `extern crate proc_macro` public, as recommended
by Rust's E0365 diagnostic, so the existing public re-export is legal.
This crate is used at compile time through GPUI's stacksafe-macro dependency.
Remove the patch when that dependency stops requiring the affected release.
