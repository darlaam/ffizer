use crate::Error;
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{Config, Cred, FetchOptions, Repository};
use git2_credentials;
use snafu::ResultExt;
use std::path::Path;

/// clone a repository at a rev to a directory
// TODO id the directory is already present then fetch and rebase (if not in offline mode)
pub fn retrieve<P, U, R>(
    dst: P,
    url: U,
    rev: R,
    credentials: Option<(&str, &str)>,
) -> Result<(), Error>
where
    P: AsRef<Path>,
    R: AsRef<str>,
    U: AsRef<str>,
{
    let dst = dst.as_ref();
    let mut fo = make_fetch_options(credentials).context(crate::GitRetrieve {
        dst: dst.to_path_buf(),
        url: url.as_ref().to_owned(),
        rev: rev.as_ref().to_owned(),
    })?;
    if dst.exists() {
        checkout(dst, &rev).context(crate::GitRetrieve {
            dst: dst.to_path_buf(),
            url: url.as_ref().to_owned(),
            rev: rev.as_ref().to_owned(),
        })?;
        pull(dst, &rev, &mut fo).context(crate::GitRetrieve {
            dst: dst.to_path_buf(),
            url: url.as_ref().to_owned(),
            rev: rev.as_ref().to_owned(),
        })?;
    //until pull is fixed and work as expected
    // let mut tmp = dst.to_path_buf().clone();
    // tmp.set_extension("part");
    // if tmp.exists() {
    //     std::fs::remove_dir_all(&tmp)?;
    // }
    // clone(&tmp, url, "master", fo)?;
    // checkout(&tmp, rev)?;
    // std::fs::remove_dir_all(&dst)?;
    // std::fs::rename(&tmp, &dst)?;
    } else {
        clone(&dst, &url, "master", fo)?;
        checkout(&dst, &rev).context(crate::GitRetrieve {
            dst: dst.to_path_buf(),
            url: url.as_ref().to_owned(),
            rev: rev.as_ref().to_owned(),
        })?;
    }
    Ok(())
}

/// a best attempt effort is made to authenticate
/// requests when required to support private
/// git repositories
fn make_fetch_options<'a>(
    credentials: Option<(&'a str, &'a str)>,
) -> Result<FetchOptions<'a>, git2::Error> {
    let mut cb = git2::RemoteCallbacks::new();

    match credentials {
        Some(creds) => {
            cb.credentials(move |_, _, _| {
                let credentials = Cred::userpass_plaintext(creds.0, creds.1)?;
                Ok(credentials)
            });
        }
        None => {
            let git_config = git2::Config::open_default()?;
            let mut ch = git2_credentials::CredentialHandler::new(git_config);
            cb.credentials(move |url, username, allowed| {
                ch.try_next_credential(url, username, allowed)
            });
        }
    }

    let mut fo = FetchOptions::new();
    let mut proxy_options = git2::ProxyOptions::new();
    proxy_options.auto();
    fo.proxy_options(proxy_options)
        .remote_callbacks(cb)
        .download_tags(git2::AutotagOption::All)
        .update_fetchhead(true);
    Ok(fo)
}

fn clone<P, U, R>(dst: P, url: U, rev: R, fo: FetchOptions<'_>) -> Result<(), Error>
where
    P: AsRef<Path>,
    R: AsRef<str>,
    U: AsRef<str>,
{
    println!(
        "dst: {}, url: {}",
        dst.as_ref().to_str().unwrap(),
        url.as_ref()
    );
    std::fs::create_dir_all(&dst.as_ref()).context(crate::CreateFolder {
        path: dst.as_ref().to_path_buf(),
    })?;
    RepoBuilder::new()
        .branch(rev.as_ref())
        .fetch_options(fo)
        .clone(url.as_ref(), dst.as_ref())
        .context(crate::GitRetrieve {
            dst: dst.as_ref().to_path_buf(),
            url: url.as_ref().to_owned(),
            rev: rev.as_ref().to_owned(),
        })?;
    Ok(())
}

// see https://stackoverflow.com/questions/54100789/how-is-git-pull-done-with-the-git2-rs-rust-crate
fn pull<'a, P, R>(dst: P, rev: R, fo: &mut FetchOptions<'a>) -> Result<(), git2::Error>
where
    P: AsRef<Path>,
    R: AsRef<str>,
{
    let repository = Repository::discover(dst.as_ref())?;

    // fetch
    let revref = rev.as_ref();
    let mut remote = repository.find_remote("origin")?;
    remote.fetch(&[revref], Some(fo), None)?;
    remote.disconnect()?;

    // merge
    let reference = repository.find_reference("FETCH_HEAD")?;
    let fetch_head_commit = repository.reference_to_annotated_commit(&reference)?;
    repository.merge(&[&fetch_head_commit], None, None)?;
    repository.cleanup_state()?;

    Ok(())
}

fn checkout<P, R>(dst: P, rev: R) -> Result<(), git2::Error>
where
    P: AsRef<Path>,
    R: AsRef<str>,
{
    let rev = rev.as_ref();
    let repository = Repository::discover(dst.as_ref())?;
    let mut co = CheckoutBuilder::new();
    co.force().remove_ignored(true).remove_untracked(true);
    let treeish = repository.revparse_single(rev)?;
    repository.checkout_tree(&treeish, Some(&mut co))?;
    Ok(())
}

/// kind can be "merge" or "diff"
pub fn find_cmd_tool(kind: &str) -> Result<String, git2::Error> {
    let config = Config::open_default()?;
    let tool = config.get_string(&format!("{}.tool", kind))?;
    config.get_string(&format!("{}tool.{}.cmd", kind, tool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use run_script;
    use std::fs;
    use tempfile::tempdir;

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn retrieve_should_update_existing_template() -> Result<(), Box<dyn std::error::Error>> {
        if std::process::Command::new("git")
            .arg("version")
            .output()
            .is_err()
        {
            eprintln!("skip the test because `git` is not installed");
            return Ok(());
        }

        let tmp_dir = tempdir()?;

        // template v1
        let src_path = tmp_dir.path().join("src");
        let options = run_script::ScriptOptions::new();
        let args = vec![];
        let (code, output, error) = run_script::run(
            &format!(
                r#"
            mkdir -p {}
            cd {}
            git init
            git config user.email "test@example.com"
            git config user.name "Test Name"
            echo "v1: Lorem ipsum" > foo.txt
            git add foo.txt
            git commit -m "add foo.txt"
            "#,
                src_path.to_str().unwrap(),
                src_path.to_str().unwrap()
            ),
            &args,
            &options,
        )?;
        if code != 0 {
            eprintln!("---output:\n{}\n---error:\n{}\n---", output, error);
        }
        assert_eq!(code, 0);

        let dst_path = tmp_dir.path().join("dst");
        retrieve(&dst_path, src_path.to_str().unwrap(), "master", None)?;
        assert_eq!(
            fs::read_to_string(&dst_path.join("foo.txt"))?,
            "v1: Lorem ipsum\n"
        );

        // template v2
        let (code, output, error) = run_script::run(
            &format!(
                r#"
            cd {}
            echo "v2: Hello" > foo.txt
            git add foo.txt
            git commit -m "add foo.txt"
            "#,
                src_path.to_str().unwrap()
            ),
            &args,
            &options,
        )?;
        if code != 0 {
            eprintln!("---output:\n{}\n---error:\n{}\n---", output, error);
        }
        assert_eq!(code, 0);

        retrieve(&dst_path, src_path.to_str().unwrap(), "master", None)?;
        assert_eq!(
            fs::read_to_string(&dst_path.join("foo.txt"))?,
            "v2: Hello\n"
        );

        // template v3
        let (code, output, error) = run_script::run(
            &format!(
                r#"
            cd {}
            echo "v3: Hourra" > foo.txt
            git add foo.txt
            git commit -m "add foo.txt"
            "#,
                src_path.to_str().unwrap()
            ),
            &args,
            &options,
        )?;
        if code != 0 {
            eprintln!("---output:\n{}\n---error:\n{}\n---", output, error);
        }
        assert_eq!(code, 0);

        retrieve(&dst_path, src_path.to_str().unwrap(), "master", None)?;
        assert_eq!(
            fs::read_to_string(&dst_path.join("foo.txt"))?,
            "v3: Hourra\n"
        );
        //TODO always remove
        fs::remove_dir_all(tmp_dir)?;
        Ok(())
    }
}
