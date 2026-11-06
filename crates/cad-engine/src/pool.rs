//! Worker threads that evaluate nodes of a [`crate::Job`] in parallel ([`crate::Engine::set_workers`];
//! native targets only).
//!
//! Each worker owns a kernel made by the factory in its own thread, so the kernel need not be
//! `Send`; shapes, node results and the tree move between threads (`K::Shape: Send + Sync`).
//! A task is one node: the job hands over its tree, the result slots (`OnceLock`s the job sets
//! as nodes finish; a task only reads its children's) and what the node's evaluation reads;
//! the worker calls [`Kernel::begin_node`] and evaluates the node without budget, in the job's
//! language, and sends the result back on the job's channel. A panic is caught and sent back as
//! well (the job resumes it). Tasks of a dropped job that have not started are skipped.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use cad_ir::{NodeIdx, Tree};
use cad_kernel::{Kernel, Tag};

use crate::eval::{self, Ctx, Run};
use crate::{EngineConfig, NodeOutput};

/// Stack of a worker thread (as the CLI's evaluation thread: solidcore recurses deeply).
const WORKER_STACK: usize = 256 << 20;

type Work<K> = Box<dyn FnOnce(&mut K) + Send>;

/// What a task of a job needs (shared between its tasks).
pub(crate) struct Shared<S> {
    pub tree: Arc<Tree>,
    pub tags: Arc<[Tag]>,
    pub outputs: Arc<[OnceLock<Arc<NodeOutput<S>>>]>,
    pub config: Arc<EngineConfig>,
    pub skip: Arc<[bool]>,
    pub lang: cad_i18n::Lang,
    /// Measure blend times (the engine has a clock).
    pub timed: bool,
    pub cancelled: Arc<AtomicBool>,
    pub results: Sender<Done<S>>,
}

impl<S> Clone for Shared<S> {
    fn clone(&self) -> Self {
        Shared {
            tree: self.tree.clone(),
            tags: self.tags.clone(),
            outputs: self.outputs.clone(),
            config: self.config.clone(),
            skip: self.skip.clone(),
            lang: self.lang,
            timed: self.timed,
            cancelled: self.cancelled.clone(),
            results: self.results.clone(),
        }
    }
}

/// One node to evaluate.
pub(crate) struct Task<S> {
    pub shared: Shared<S>,
    pub idx: NodeIdx,
    pub tainted: bool,
    pub seed: u64,
}

/// The result of a task: the node's output (or the panic of its evaluation) and its kernel
/// calls.
pub(crate) struct Done<S> {
    pub idx: NodeIdx,
    pub out: Result<NodeOutput<S>, Box<dyn Any + Send>>,
    pub ops: u64,
}

/// A job's end of the result channel.
pub(crate) fn results<S>() -> (Sender<Done<S>>, Receiver<Done<S>>) {
    channel()
}

pub(crate) struct Pool<K: Kernel> {
    tx: Option<Sender<Work<K>>>,
    handles: Vec<JoinHandle<()>>,
    /// Boxes a task for the workers; made where `K::Shape: Send + Sync` holds
    /// ([`Pool::new`]), so the job's generic code can submit without that bound.
    submit: fn(&Pool<K>, Task<K::Shape>),
}

impl<K: Kernel + 'static> Pool<K> {
    /// `n` worker threads, each with a kernel from `make`.
    pub(crate) fn new(n: usize, make: Arc<dyn Fn() -> K + Send + Sync>) -> Self
    where
        K::Shape: Send + Sync + 'static,
    {
        let (tx, rx) = channel::<Work<K>>();
        let rx = Arc::new(Mutex::new(rx));
        let handles = (0..n)
            .filter_map(|i| {
                let rx = rx.clone();
                let make = make.clone();
                std::thread::Builder::new()
                    .name(format!("cad-worker-{i}"))
                    .stack_size(WORKER_STACK)
                    .spawn(move || {
                        let mut k = make();
                        loop {
                            let work = { rx.lock().unwrap_or_else(|e| e.into_inner()).recv() };
                            match work {
                                Ok(w) => w(&mut k),
                                Err(_) => break,
                            }
                        }
                    })
                    .ok()
            })
            .collect();
        Pool { tx: Some(tx), handles, submit: submit_task::<K> }
    }
}

impl<K: Kernel> Pool<K> {
    pub(crate) fn workers(&self) -> usize {
        self.handles.len()
    }

    pub(crate) fn submit(&self, t: Task<K::Shape>) {
        (self.submit)(self, t)
    }
}

impl<K: Kernel> Drop for Pool<K> {
    fn drop(&mut self) {
        // the workers end when the channel closes (after their current task)
        self.tx.take();
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

fn submit_task<K: Kernel + 'static>(pool: &Pool<K>, t: Task<K::Shape>)
where
    K::Shape: Send + Sync + 'static,
{
    let work: Work<K> = Box::new(move |k: &mut K| {
        let Task { shared, idx, tainted, seed } = t;
        if shared.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let _lang = cad_i18n::scoped(shared.lang);
        let t0 = std::time::Instant::now();
        let clock = move || t0.elapsed().as_secs_f64() * 1000.0;
        let mut replay = Vec::new();
        let mut ops = 0u64;
        if tainted {
            k.set_draft(true);
        }
        let out = std::panic::catch_unwind(AssertUnwindSafe(|| {
            k.begin_node(seed);
            let mut run = Run {
                k: &mut *k,
                replay: &mut replay,
                cursor: 0,
                ops: &mut ops,
                max_ops: None,
                deadline: None,
                clock: if shared.timed { Some(&clock) } else { None },
            };
            let ctx = Ctx {
                tree: &shared.tree,
                tags: &shared.tags,
                outputs: &shared.outputs,
                config: &shared.config,
                skip: &shared.skip,
            };
            eval::eval_node(&mut run, &ctx, idx).unwrap_or_else(|_| unreachable!("no budget, no interrupt"))
        }));
        if tainted {
            k.set_draft(false);
        }
        let _ = shared.results.send(Done { idx, out, ops });
    });
    if let Some(tx) = &pool.tx {
        let _ = tx.send(work);
    }
}
