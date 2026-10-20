# Third-party notices

This repository is licensed under GPL-3.0-or-later. Some source files contain code or data derived from the projects
below. Their licences are compatible with GPL-3.0 and their notices are reproduced or referenced here.

## glibc 2.19 (LGPL-2.1-or-later)

`crates/blockheads_server/src/fmath/libm.rs` and `app/getopt.rs` reproduce the observable behaviour of glibc 2.19
(`sinf`/`cosf` for x86-64, `tanf`, `powf`, `atan2f`, the IBM Accurate Mathematical Library results for the `f64`
functions, and `getopt_long_only`) bit for bit, including some tables and the argument permutation algorithm.
Copyright (C) the Free Software Foundation, Inc. Licensed under the GNU Lesser General Public License, version 2.1
or (at your option) any later version.

## fdlibm (Sun Microsystems permissive notice)

The single-precision routines in `fmath/libm.rs` use tables and reduction constants from fdlibm
(`TWO_OVER_PI`, `NPIO2_HW`, `PIO4J`, `FPI`).

> Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
>
> Developed at SunPro, a Sun Microsystems, Inc. business. Permission to use, copy, modify, and distribute this
> software is freely granted, provided that this notice is preserved.

## GNUstep-base 1.24 (LGPL-2.1-or-later)

`foundation/nsstring.rs` (Unicode decomposition and case tables) and `foundation/gsdict.rs` (the `GSIMapTable`
enumeration order) reproduce the behaviour and tables of GNUstep-base 1.24.
Copyright (C) the GNUstep project. Licensed under the GNU Lesser General Public License, version 2.1 or (at your
option) any later version.

## Rust crates

Dependencies (rusty_enet, heed, lmdb-master-sys, flate2, libz-sys, md-5, base64, socket2, libc, chrono, ...) are used
under their own licences; see `Cargo.lock` and each crate's metadata.
