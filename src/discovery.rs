use crate::{
    config::Config,
    error::Error,
    github::GithubClient,
    model::{Project, Projects},
};

#[derive(Clone)]
pub struct Discovery {
    pub(crate) config: Config,
    pub(crate) github: GithubClient,
}

impl Discovery {
    pub fn new(config: Config, github: GithubClient) -> Result<Self, Error> {
        config.validate()?;
        Ok(Self { config, github })
    }

    pub async fn list_projects(&self) -> Result<Projects, Error> {
        let config = &self.config.github;
        let repositories = self.github.repositories(&config.owner).await?;
        let mut projects: Vec<Project> = repositories
            .into_iter()
            .filter(|r| {
                !r.private
                    && (config.include_archived || !r.archived)
                    && r.topics.contains(&config.topic)
            })
            .map(Project::from)
            .collect();
        projects.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        projects.dedup_by(|a, b| a.full_name == b.full_name);
        Ok(Projects { projects })
    }
}
