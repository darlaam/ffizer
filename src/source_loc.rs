use crate::git;
use crate::source_uri::SourceUri;
use crate::transform_values::TransformsValues;
use crate::Ctx;
use crate::Result;
use slog::warn;
use snafu::ResultExt;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(
    StructOpt, Debug, Default, Clone, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Builder,
)]
#[serde(deny_unknown_fields, default)]
#[builder(default)]
#[builder(field(private))]
#[builder(setter(into, strip_option))]
pub struct SourceLoc {
    /// uri / path of the template
    #[structopt(short = "s", long = "source")]
    pub uri: SourceUri,

    /// git revision of the template
    #[structopt(long = "rev", default_value = "master")]
    pub rev: String,

    /// path of the folder under the source uri to use for template
    #[structopt(long = "source-subfolder", parse(from_os_str))]
    pub subfolder: Option<PathBuf>,
}

impl SourceLoc {
    pub fn builder() -> SourceLocBuilder {
        SourceLocBuilder::default()
    }

    pub fn find_remote_cache_folder() -> Result<PathBuf> {
        let app_name = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "".into());
        let project_dirs = directories::ProjectDirs::from("", &app_name, &app_name)
            .ok_or(crate::Error::ApplicationPathNotFound {})?;
        let cache_base = project_dirs.cache_dir();
        Ok(cache_base.join("git"))
    }

    pub fn as_local_path(&self) -> Result<PathBuf> {
        let mut path = match self.uri.host {
            None => self.uri.path.canonicalize().context(crate::error::Io {})?,
            Some(_) => self.remote_as_local()?,
        };
        if let Some(f) = &self.subfolder {
            path = path.join(f.clone());
        }
        Ok(path)
    }

    // the remote_as_local ignore subfolder
    fn remote_as_local(&self) -> Result<PathBuf> {
        let cache_uri = Self::find_remote_cache_folder()?
            .join(
                &self
                    .uri
                    .host
                    .clone()
                    .unwrap_or_else(|| "no_host".to_owned()),
            )
            .join(&self.uri.path)
            .join(&self.rev);
        Ok(cache_uri)
    }

    pub fn download(&self, ctx: &Ctx, offline: bool) -> Result<PathBuf> {
        if !offline && self.uri.host.is_some() {
            let remote_path = self.remote_as_local()?;
            if let Err(v) = git::retrieve(&remote_path, &self.uri.raw, &self.rev) {
                warn!(ctx.logger, "failed to download"; "src" => ?&self, "path" => ?&remote_path, "error" => ?&v);
                if remote_path.exists() {
                    fs::remove_dir_all(&remote_path)
                        .context(crate::RemoveFolder { path: remote_path })?;
                }
                return Err(v);
            }
        }
        let path = self.as_local_path()?;
        if !path.exists() {
            Err(crate::Error::LocalPathNotFound {
                path,
                uri: self.uri.raw.clone(),
                subfolder: self.subfolder.clone(),
            })
        } else {
            Ok(path)
        }
    }
}

impl TransformsValues for SourceLoc {
    /// transforms default_value & ignore
    fn transforms_values<F>(&self, render: &F) -> Result<SourceLoc>
    where
        F: Fn(&str) -> String,
    {
        let uri = self.uri.transforms_values(render)?;
        let rev = render(&self.rev);
        let subfolder = self
            .subfolder
            .clone()
            .and_then(|f| f.transforms_values(render).ok());
        Ok(SourceLoc {
            uri,
            rev,
            subfolder,
        })
    }
}

impl fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (rev: {}{})",
            self.uri.raw,
            self.rev,
            self.subfolder
                .as_ref()
                .map(|s| format!(", subfolder: {}", s.to_string_lossy()))
                .unwrap_or("".to_string())
        )
    }
}
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use spectral::prelude::*;
//     use crate::source_uri::SourceUri;
//     use std::str::FromStr;

//     #[test]
//     fn as_local_path_on_git() -> Result<()> {
//         let sut = SourceLoc {
//             uri: SourceUri::from_str("git@github.com:ffizer/ffizer.git")?,
//             rev: "master".to_owned(),
//             subfolder: None,
//         };
//         assert_that!(&sut.as_local_path().unwrap()).ends_with("/com.github.ffizer/git/github.com/ffizer/ffizer/master");
//         Ok(())
//     }
// }
