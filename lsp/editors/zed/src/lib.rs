use zed::settings::{CommandSettings, LspSettings};
use zed_extension_api as zed;

struct FibExtension;

impl zed::Extension for FibExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        server_command(
            settings.binary,
            |name| worktree.which(name),
            worktree.shell_env(),
        )
    }
}

fn server_command(
    binary: Option<CommandSettings>,
    mut which: impl FnMut(&str) -> Option<String>,
    mut env: zed::EnvVars,
) -> zed::Result<zed::Command> {
    let binary = binary.unwrap_or(CommandSettings {
        path: None,
        arguments: None,
        env: None,
    });
    let command = match binary.path {
        Some(path) => path,
        None => which("fib-lsp").ok_or_else(|| {
            "fib-lsp not found on the worktree PATH. Install Node.js 20+ and run `npm install --global /path/to/fib/lsp`, or configure lsp.fib-lsp.binary with a Node executable and the absolute path to lsp/src/server.js."
                .to_string()
        })?,
    };
    if let Some(overrides) = binary.env {
        env.retain(|(key, _)| !overrides.contains_key(key));
        env.extend(overrides);
    }
    Ok(zed::Command {
        command,
        args: binary.arguments.unwrap_or_default(),
        env,
    })
}

zed::register_extension!(FibExtension);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_installed_server_on_worktree_path() {
        let command = server_command(
            None,
            |name| {
                assert_eq!(name, "fib-lsp");
                Some("/npm/bin/fib-lsp".into())
            },
            vec![("PATH".into(), "/npm/bin:/usr/bin".into())],
        )
        .unwrap();
        assert_eq!(command.command, "/npm/bin/fib-lsp");
        assert!(command.args.is_empty());
        assert_eq!(command.env[0].1, "/npm/bin:/usr/bin");
    }

    #[test]
    fn supports_explicit_node_script_and_environment() {
        let command = server_command(
            Some(CommandSettings {
                path: Some("/usr/bin/node".into()),
                arguments: Some(vec!["/checkout with spaces/fib/lsp/src/server.js".into()]),
                env: Some([("NODE_OPTIONS".into(), "--no-warnings".into())].into()),
            }),
            |_| panic!("explicit binary must not require fib-lsp on PATH"),
            vec![
                ("PATH".into(), "/usr/bin".into()),
                ("NODE_OPTIONS".into(), "old".into()),
            ],
        )
        .unwrap();
        assert_eq!(command.command, "/usr/bin/node");
        assert_eq!(
            command.args,
            ["/checkout with spaces/fib/lsp/src/server.js"]
        );
        assert_eq!(command.env.len(), 2);
        assert!(
            command
                .env
                .contains(&("NODE_OPTIONS".into(), "--no-warnings".into()))
        );
        assert!(command.env.contains(&("PATH".into(), "/usr/bin".into())));
    }

    #[test]
    fn missing_server_reports_installation_and_override_options() {
        let error = server_command(None, |_| None, vec![]).unwrap_err();
        assert!(error.contains("fib-lsp not found"));
        assert!(error.contains("npm install --global"));
        assert!(error.contains("lsp.fib-lsp.binary"));
    }

    #[test]
    fn arguments_without_path_still_use_path_lookup() {
        let command = server_command(
            Some(CommandSettings {
                path: None,
                arguments: Some(vec!["--example".into()]),
                env: None,
            }),
            |_| Some("/bin/fib-lsp".into()),
            vec![],
        )
        .unwrap();
        assert_eq!(command.command, "/bin/fib-lsp");
        assert_eq!(command.args, ["--example"]);
    }
}
