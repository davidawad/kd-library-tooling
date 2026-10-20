# Private dual-host plan (not executed)

Replace `OWNER` with the GitLab and GitHub namespace David approves. These commands create private repositories and mirror the same branch and tag; they have not been run.

```sh
glab repo create OWNER/kd-library-tooling --private --defaultBranch main
gh repo create OWNER/kd-library-tooling --private

git remote add gitlab git@gitlab.com:OWNER/kd-library-tooling.git
git remote add github git@github.com:OWNER/kd-library-tooling.git

git push gitlab main --tags
git push github main --tags

git ls-remote gitlab refs/heads/main refs/tags/v0.1.0
git ls-remote github refs/heads/main refs/tags/v0.1.0
```

Compare the `main` and `v0.1.0` object IDs from both `git ls-remote` outputs to verify parity. No remotes have been created and no commands above were executed.
