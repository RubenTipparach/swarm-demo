---
name: tidy
description: Run every check a push to swarm-demo must pass (the shape of the code, rustfmt, clippy, the dash grep, the core suite) and report what fails, then review the diff for reuse and simplification. Use before any push, and whenever asked whether the code is clean.
---

# Tidy

The rules are in `CLAUDE.md` under "How the code is written". This runs
them. From the repository root, in this order, and report every result
plainly: a failure is named with the line the tool printed, never summarised
as "some warnings".

1. **The shape.** `python3 tools/shape.py --check`. A file over 900 lines or
   a function over 100 fails. The offenders it lists are the work, not a
   number to raise the limit past.
2. **The format.** `cargo fmt --all -- --check`. If it fails, `cargo fmt
   --all` and commit the formatting on its own, with nothing else in the
   commit, so `git blame` stays readable.
3. **The lints.** `cargo clippy -p swarm_core -- -D warnings`, which must be
   clean, and `cargo clippy -p swarm_app`, whose warning count is reported
   and must not go up.
4. **The dashes.** `LC_ALL=C.UTF-8 git ls-files -z | xargs -0 grep -lP
   '[\x{2013}\x{2014}]'` must print nothing.
5. **The core suite.** `cargo test -p swarm_core`.
6. **The review.** Run `/simplify` on the diff for reuse, simplification and
   altitude, and `/code-review` for correctness. Apply what they find before
   the push, not after.

Then say, in one line each, what passed, what failed, and what was changed.
