use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "tend-ship ship",
    about = "Commit and push using the current Claude Code session's transcript",
    disable_version_flag = true
)]
pub struct ShipArgs {
    /// Target worktree to ship (path or basename); defaults to cwd
    #[arg(value_name = "WORKTREE")]
    pub worktree: Option<String>,

    /// Use a specific session JSONL instead of newest-for-target
    #[arg(short = 's', long = "session", value_name = "ID")]
    pub session: Option<String>,

    /// Skip session reading; use this text as the commit subject
    #[arg(short = 'm', long = "message", value_name = "TEXT")]
    pub message: Option<String>,

    /// Skip confirmation; ship the auto-picked candidate
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Commit but don't push
    #[arg(long = "no-push")]
    pub no_push: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_none_and_false() {
        let args = ShipArgs::try_parse_from(["ship"]).unwrap();
        assert_eq!(args.session, None);
        assert_eq!(args.message, None);
        assert!(!args.force);
        assert!(!args.no_push);
    }

    #[test]
    fn parses_force_short_and_long() {
        let short = ShipArgs::try_parse_from(["ship", "-f"]).unwrap();
        assert!(short.force);
        let long = ShipArgs::try_parse_from(["ship", "--force"]).unwrap();
        assert!(long.force);
    }

    #[test]
    fn parses_message() {
        let args = ShipArgs::try_parse_from(["ship", "-m", "custom subject"]).unwrap();
        assert_eq!(args.message.as_deref(), Some("custom subject"));
        let args = ShipArgs::try_parse_from(["ship", "--message", "another"]).unwrap();
        assert_eq!(args.message.as_deref(), Some("another"));
    }

    #[test]
    fn parses_session_id() {
        let args = ShipArgs::try_parse_from(["ship", "-s", "abc-123"]).unwrap();
        assert_eq!(args.session.as_deref(), Some("abc-123"));
    }

    #[test]
    fn parses_no_push() {
        let args = ShipArgs::try_parse_from(["ship", "--no-push"]).unwrap();
        assert!(args.no_push);
    }

    #[test]
    fn parses_all_flags_together() {
        let args = ShipArgs::try_parse_from([
            "ship", "-f", "--no-push", "-s", "xyz", "-m", "subj",
        ])
        .unwrap();
        assert!(args.force);
        assert!(args.no_push);
        assert_eq!(args.session.as_deref(), Some("xyz"));
        assert_eq!(args.message.as_deref(), Some("subj"));
    }

    #[test]
    fn rejects_unknown_flag() {
        let result = ShipArgs::try_parse_from(["ship", "--bogus"]);
        assert!(result.is_err());
    }
}
