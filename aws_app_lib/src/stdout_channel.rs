use anyhow::Error;
use deadqueue::unlimited::Queue;
use std::sync::Arc;
use tokio::{
    io::{stderr, stdout, AsyncWriteExt},
    sync::Mutex,
    task::{spawn, JoinHandle},
};
use stack_string::StackString;

enum MessageType {
    Mesg(StackString),
    Close,
}

type ChanType = Queue<MessageType>;

#[derive(Clone)]
pub struct StdoutChannel {
    stdout_queue: Arc<ChanType>,
    stderr_queue: Arc<ChanType>,
    stdout_task: Arc<Mutex<Option<JoinHandle<Result<(), Error>>>>>,
    stderr_task: Arc<Mutex<Option<JoinHandle<Result<(), Error>>>>>,
}

impl Default for StdoutChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl StdoutChannel {
    pub fn new() -> Self {
        let stdout_queue = Arc::new(Queue::new());
        let stderr_queue = Arc::new(Queue::new());
        let stdout_task = {
            let queue = stdout_queue.clone();
            Arc::new(Mutex::new(Some(spawn(async move {
                Self::stdout_task(&queue).await
            }))))
        };
        let stderr_task = {
            let queue = stderr_queue.clone();
            Arc::new(Mutex::new(Some(spawn(async move {
                Self::stderr_task(&queue).await
            }))))
        };
        Self {
            stdout_queue,
            stderr_queue,
            stdout_task,
            stderr_task,
        }
    }

    pub fn send<T: Into<StackString>>(&self, item: T) {
        self.stdout_queue.push(MessageType::Mesg(item.into()));
    }

    pub fn send_err<T: Into<StackString>>(&self, item: T) {
        self.stderr_queue.push(MessageType::Mesg(item.into()));
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.stdout_queue.push(MessageType::Close);
        self.stderr_queue.push(MessageType::Close);
        if let Some(stdout_task) = self.stdout_task.lock().await.take() {
            stdout_task.await??;
        }
        if let Some(stderr_task) = self.stderr_task.lock().await.take() {
            stderr_task.await??;
        }
        Ok(())
    }

    async fn stdout_task(queue: &ChanType) -> Result<(), Error> {
        while let MessageType::Mesg(line) = queue.pop().await {
            stdout().write_all(format!("{}\n", line).as_bytes()).await?;
        }
        Ok(())
    }

    async fn stderr_task(queue: &ChanType) -> Result<(), Error> {
        while let MessageType::Mesg(line) = queue.pop().await {
            stderr().write_all(format!("{}\n", line).as_bytes()).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::stdout_channel::StdoutChannel;

    #[tokio::test]
    async fn test_stdout_task() -> Result<(), Error> {
        let chan = StdoutChannel::new();

        chan.send("stdout: Hey There");
        chan.send("What's happening".to_string());
        chan.send_err("stderr: How it goes");
        chan.close().await?;

        Ok(())
    }
}
