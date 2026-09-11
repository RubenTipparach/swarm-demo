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
4. **The dashes.** `git ls-files -z | LC_ALL=C.UTF-8 xargs -0 grep -lP
   '[\x{2013}\x{2014}]'` must print nothing. The locale is on the grep: on
   git alone, grep refuses the code points and prints an error instead of
   a file, which reads as a pass.
5. **The core suite.** `cargo test -p swarm_core`.
6. **The review.** Run `/simplify` on the diff for reuse, simplification and
   altitude, and `/code-review` for correctness. Apply what they find before
   the push, not after.
7. **The pictures, when the change claims to change nothing.** A refactor is
   proved by its renders: copy the binary from before aside, build the one
   from after, take the suite's headless shots on both, and `python3
   tools/pngdiff.py before.png after.png` on each pair. A scene with no
   kills in it is byte identical run to run and `cmp` is its check; a scene
   with kills is not, because the spark and shock rings are claimed with
   atomics and the swarm diverges from the order the threads arrived in, so
   each such pair is held against that scene's own floor, two runs of the
   before binary, and never against a number typed here.

Then say, in one line each, what passed, what failed, and what was changed.
