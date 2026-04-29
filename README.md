# The Sek Shell

The `sek` is a shell without script support.

## Why is it called `sek`?

`sek` is 石 (the stone in chinese).

The name comes from ancient chinese philosophical discussions: 离坚白,
hardness and white color are two attributes of the stone,
The philosopher 公孙龙 tried to discuss them separately.

## Why `sek`?

Just as a stone has two attributes: hardness and white color,
the shell also has two modes of use: interactive and script.

Modern shells have become powerful and complex,
and a large part of this complexity comes from script support.

The posix shell has a lot of historical problems,
but shell scripts that are not compatible with posix can hardly be successful.
It turns out that python or nodejs is a better script language than (any) shell script.

So `sek` choose not to provide any form of script support,
and instead focused on being an interactive shell.

## What's difference?

Since we gave up script support, we can make the shell syntax simpler and stricter.

Please note that the following decisions are entirely based on my experience.
It does not apply to everyone.

### The first argument must be a literal

In some scripts, the usage of `$CC ab.c` is very common,
but it is meaningless in interactive mode.
and if you forget to set environment variable, it will cause an error.

### No `if` and `for` support

I almost never use them in interactive mode.
But `sek` supports `&&` and `||`, so it can make some simple logical.

### No glob support

The glob expansion is the famous footgun of shell.
For example, you cannot use `rm *` to delete `--help` files in the directory.
In addition, it is not suitable for directory with too many files.

And this is not a common feature, the only scene I can recall is `rm *.jpg`,
I can replace it with 10L rust code.

```rust
//! rm-glob '*.jpg'
use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let pat = std::env::args().nth(1).context("no glob pattern")?;
    for path in glob::glob(&pat)? {
        std::fs::remove_file(path?)?;
    }
    Ok(())
}
```

### No environment variable (and subshell) expand

Like glob, this is also a footgun function.

It is important in script, but I have never used it in interactive mode.

### No unpopular syntax

There are some almost quirky syntax in shell,
and most users don't know its existence.

for example, `> fd ls` is a legal bash command.
It writes the output of ls to a file.

## Feature?

The `sek` does not try to create a great ecosystem.
On the contrary, it is designed to satisfy my personal use.

+ [x] Cross platform
+ [x] Basic shell command
+ [x] Basic helix mode
+ [x] Local History
+ [x] Path selector
+ [ ] Completion UI
+ [ ] History-based Completion (daemon)
+ [ ] Rule-based Completion (daemon)
+ [ ] Prompt
