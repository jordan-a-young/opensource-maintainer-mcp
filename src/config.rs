use serde::Deserialize;

use crate::error::Error;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub github: GithubConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    pub owner: String,
    pub topic: String,
    #[serde(default)]
    pub include_archived: bool,
}

impl Config {
    pub fn parse(input: &str) -> Result<Self, Error> {
        // Do not include TOML source in errors: it may contain misplaced secrets.
        let config: Self = toml::from_str(input).map_err(|_| {
            Error::Config("expected [github] with owner, topic, and optional include_archived")
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Error> {
        let owner = &self.github.owner;
        if owner.is_empty()
            || owner.len() > 39
            || !owner
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || owner.starts_with('-')
            || owner.ends_with('-')
            || owner.contains("--")
        {
            return Err(Error::Config(
                "owner must be a valid GitHub user or organization login",
            ));
        }
        let topic = &self.github.topic;
        if topic.is_empty()
            || topic.len() > 50
            || !topic
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(Error::Config(
                "topic must contain 1–50 lowercase letters, digits, or hyphens",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_and_defaults() {
        let config = Config::parse(include_str!("../examples/config.toml")).expect("valid example");
        assert_eq!(config.github.owner, "availity");
        assert!(!config.github.include_archived);
        assert!(
            !Config::parse("[github]\nowner='octocat'\ntopic='rust'")
                .expect("defaults")
                .github
                .include_archived
        );
    }

    #[test]
    fn rejects_invalid_and_unknown_configuration() {
        for input in [
            "",
            "[github]",
            "[github]\nowner='../x'\ntopic='rust'",
            "[github]\nowner='x'\ntopic=''",
            "[github]\nowner='x'\ntopic='Rust'",
            "[github]\nowner='x'\ntopic='rust'\ntoken='secret'",
            "[github]\nowner='x'\ntopic='rust'\ninclude_archived='false'",
        ] {
            let error = Config::parse(input)
                .expect_err("invalid config")
                .to_string();
            assert!(!error.contains("secret"));
        }
    }
}
