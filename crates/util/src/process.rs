use anyhow::{Context as _, Result};
use std::process::Stdio;

use crate::ResultExt as _;

/// A wrapper around `smol::process::Child` that ensures all subprocesses
/// are killed when the process is terminated by using process groups.
pub struct Child {
    process: Option<smol::process::Child>,
}

impl std::ops::Deref for Child {
    type Target = smol::process::Child;

    fn deref(&self) -> &Self::Target {
        self.process.as_ref().expect("process already consumed")
    }
}

impl std::ops::DerefMut for Child {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.process.as_mut().expect("process already consumed")
    }
}

impl Child {
    #[cfg(not(windows))]
    pub fn spawn(
        mut command: std::process::Command,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<Self> {
        crate::set_pre_exec_to_start_new_session(&mut command);
        let mut command = smol::process::Command::from(command);
        let process = command
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn command {}",
                    crate::redact::redact_command(&format!("{command:?}"))
                )
            })?;
        Ok(Self {
            process: Some(process),
        })
    }

    #[cfg(windows)]
    pub fn spawn(
        command: std::process::Command,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<Self> {
        // TODO(windows): create a job object and add the child process handle to it,
        // see https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
        let mut command = smol::process::Command::from(command);
        let process = command
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn command {}",
                    crate::redact::redact_command(&format!("{command:?}"))
                )
            })?;

        Ok(Self {
            process: Some(process),
        })
    }

    /// Consumes this `Child`, returning the inner `smol::process::Child` without killing it.
    pub fn into_inner(mut self) -> smol::process::Child {
        self.process.take().expect("process already consumed")
    }

    #[cfg(not(windows))]
    pub fn kill(&mut self) -> Result<()> {
        let Some(process) = &mut self.process else {
            return Ok(());
        };
        let pid = process.id();
        unsafe {
            libc::killpg(pid as i32, libc::SIGKILL);
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn kill(&mut self) -> Result<()> {
        let Some(process) = &mut self.process else {
            return Ok(());
        };
        // TODO(windows): terminate the job object in kill
        process.kill()?;
        Ok(())
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        self.kill().log_err();
    }
}
