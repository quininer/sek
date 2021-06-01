use bumpalo::Bump;
use bumpalo::collections::String;
use crate::shell::{ Shell, Action };
use crate::editor::{ Editor, Mode };


pub const EDITOR_COMMANDS: &[(&str, CommandFn)] = &[
    ("q", quit),
    ("quit", quit),
    ("alias-expand", alias_expand),
];

pub const PATH_SELECTOR_COMMANDS: &[(&str, CommandFn)] = &[
    ("filter", filter),
];

type CommandFn = for<'a> fn(&'a Bump, &'a mut Editor, &'a mut Shell, Option<&'a str>)
    -> anyhow::Result<Action>;

pub fn execute_command(editor: &mut Editor, shell: &mut Shell) -> anyhow::Result<Action> {
    let bump = shell.bump.clone();
    let bump = bump.borrow();

    let mut buf = String::with_capacity_in(editor.cmd.len(), &bump);
    editor.cmd.read_into(&mut buf);
    editor.cmd.clear();
    let buf = buf.trim_start_matches(':').trim();
    let (name, arg) = buf.split_once(' ')
        .map(|(name, arg)| (name, Some(arg)))
        .unwrap_or((buf, None));

    match editor.mode {
        Mode::Normal => for &(fnname, editfn) in EDITOR_COMMANDS {
            if name == fnname {
                return editfn(&bump, editor, shell, arg);
            }
        },
        Mode::PathSelector => for &(fnname, editfn) in PATH_SELECTOR_COMMANDS {
            if name == fnname {
                return editfn(&bump, editor, shell, arg);
            }

        },
        _ => ()
    }

    Ok(Action::Continue)
}

fn quit(_bump: &Bump, _editor: &mut Editor, _shell: &mut Shell, _arg: Option<&str>)
    -> anyhow::Result<Action>
{
    Ok(Action::Stop)
}

fn alias_expand(bump: &Bump, editor: &mut Editor, shell: &mut Shell, _arg: Option<&str>)
    -> anyhow::Result<Action>
{
    let mut buf = String::with_capacity_in(editor.line.len() + 16, bump);
    editor.line.read_into(&mut buf);
    shell.alias.replace(&mut buf);
    editor.line.clear();
    for c in buf.chars() {
        editor.line.push(c);
    }
    editor.line.move_end();
    Ok(Action::Continue)
}

fn filter(_bump: &Bump, editor: &mut Editor, _shell: &mut Shell, arg: Option<&str>)
    -> anyhow::Result<Action>
{
    editor.path_selector.set_glob(if let Some(rule) = arg {
        Some(glob::Pattern::new(rule)?)
    } else {
        None
    });
    editor.path_selector.cd(".".as_ref())?;

    Ok(Action::Continue)
}
