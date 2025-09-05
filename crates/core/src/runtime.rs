// Copyright 2025 ScopeDB <contact@scopedb.io>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::future::Future;
use std::panic::resume_unwind;
use std::sync::Arc;
use std::task::ready;
use std::time::Duration;
use std::time::Instant;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

pub fn make_runtime(runtime_name: &str, thread_name: &str, worker_threads: usize) -> Runtime {
    log::info!(
        "creating runtime with runtime_name: {runtime_name}, thread_name: {thread_name}, work_threads: {worker_threads}"
    );

    Builder::new(runtime_name, thread_name)
        .worker_threads(worker_threads)
        .build()
        .expect("failed to create runtime")
}

/// A runtime to run future tasks.
#[derive(Debug, Clone)]
pub struct Runtime {
    name: String,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl Runtime {
    /// Get the name of the runtime.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Spawn a future and execute it in this thread pool.
    ///
    /// Similar to [`tokio::runtime::Runtime::spawn`].
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        JoinHandle::new(self.runtime.spawn(future))
    }

    /// Run the provided function on an executor dedicated to blocking operations.
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        JoinHandle::new(self.runtime.spawn_blocking(func))
    }

    /// Run a future to complete, this is the runtime entry point.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }
}

impl fastimer::Spawn for Runtime {
    fn spawn<F: Future<Output = ()> + Send + 'static>(&self, future: F) {
        Runtime::spawn(self, future);
    }
}

#[derive(Debug)]
struct Timer;

pub fn timer() -> impl fastimer::MakeDelay {
    Timer
}

impl fastimer::MakeDelay for Timer {
    type Delay = tokio::time::Sleep;

    fn delay_util(&self, at: Instant) -> Self::Delay {
        tokio::time::sleep_until(tokio::time::Instant::from_std(at))
    }

    fn delay(&self, duration: Duration) -> Self::Delay {
        tokio::time::sleep(duration)
    }
}

#[pin_project::pin_project]
#[derive(Debug)]
pub struct JoinHandle<R> {
    #[pin]
    inner: tokio::task::JoinHandle<R>,
}

impl<R> JoinHandle<R> {
    fn new(inner: tokio::task::JoinHandle<R>) -> Self {
        Self { inner }
    }
}

impl<R> Future for JoinHandle<R> {
    type Output = R;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        let val = ready!(this.inner.poll(cx));
        match val {
            Ok(val) => std::task::Poll::Ready(val),
            Err(err) => {
                if err.is_panic() {
                    resume_unwind(err.into_panic())
                } else {
                    unreachable!()
                }
            }
        }
    }
}

pub struct Builder {
    runtime_name: String,
    thread_name: String,
    builder: tokio::runtime::Builder,
}

impl Builder {
    /// Creates a new Builder with names.
    pub fn new(runtime_name: impl Into<String>, thread_name: impl Into<String>) -> Self {
        Self {
            runtime_name: runtime_name.into(),
            thread_name: thread_name.into(),
            builder: tokio::runtime::Builder::new_multi_thread(),
        }
    }

    /// Sets the number of worker threads the Runtime will use.
    ///
    /// This can be any number above 0. The default value is the number of cores available to the
    /// system.
    pub fn worker_threads(&mut self, val: usize) -> &mut Self {
        self.builder.worker_threads(val);
        self
    }

    /// Specifies the limit for additional threads spawned by the Runtime.
    ///
    /// These threads are used for blocking operations like tasks spawned through spawn_blocking,
    /// they are not always active and will exit if left idle for too long, You can change this
    /// timeout duration with thread_keep_alive. The default value is 512.
    pub fn max_blocking_threads(&mut self, val: usize) -> &mut Self {
        self.builder.max_blocking_threads(val);
        self
    }

    /// Sets a custom timeout for a thread in the blocking pool.
    ///
    /// By default, the timeout for a thread is set to 10 seconds.
    pub fn thread_keep_alive(&mut self, duration: Duration) -> &mut Self {
        self.builder.thread_keep_alive(duration);
        self
    }

    pub fn runtime_name(&mut self, val: impl Into<String>) -> &mut Self {
        self.runtime_name = val.into();
        self
    }

    /// Sets name of threads spawned by the Runtime thread pool
    pub fn thread_name(&mut self, val: impl Into<String>) -> &mut Self {
        self.thread_name = val.into();
        self
    }

    pub fn build(&mut self) -> std::io::Result<Runtime> {
        let name = self.runtime_name.clone();
        let runtime = self
            .builder
            .enable_all()
            .thread_name(self.thread_name.clone())
            .build()
            .map(Arc::new)?;
        Ok(Runtime { name, runtime })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

    fn runtime() -> Arc<Runtime> {
        let runtime = Builder::new("test_runtime", "test_thread")
            .worker_threads(2)
            .build();
        Arc::new(runtime.unwrap())
    }

    #[test]
    fn test_block_on() {
        let runtime = runtime();

        let out = runtime.block_on(async {
            let (tx, rx) = oneshot::channel();

            let _ = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                tx.send("ZONE").unwrap();
            });

            rx.await.unwrap()
        });

        assert_eq!(out, "ZONE");
    }

    #[test]
    fn test_spawn_blocking() {
        let runtime = runtime();
        let runtime1 = runtime.clone();
        let out = runtime.block_on(async move {
            let runtime2 = runtime1.clone();
            let inner = runtime1
                .spawn_blocking(move || runtime2.spawn(async move { "hello" }))
                .await;

            inner.await
        });

        assert_eq!(out, "hello")
    }

    #[test]
    fn test_spawn_join() {
        let runtime = runtime();
        let handle = runtime.spawn(async { 1 + 1 });

        assert_eq!(2, runtime.block_on(handle));
    }
}
