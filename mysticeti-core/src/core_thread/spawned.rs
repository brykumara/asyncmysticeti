// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block_handler::BlockHandler;
use crate::data::Data;
use crate::metrics::{Metrics, UtilizationTimerExt};
use crate::syncer::{CommitObserver, Syncer, SyncerSignals};
use crate::types::{RoundNumber, StatementBlock};
#[cfg(unix)]
use std::os::unix::thread::JoinHandleExt;
use std::sync::Arc;
use std::thread;
use tokio::sync::{mpsc, oneshot};

pub struct CoreThreadDispatcher<H: BlockHandler, S: SyncerSignals, C: CommitObserver> {
    sender: mpsc::Sender<CoreThreadCommand>,
    join_handle: thread::JoinHandle<Syncer<H, S, C>>,
    metrics: Arc<Metrics>,
}

pub struct CoreThread<H: BlockHandler, S: SyncerSignals, C: CommitObserver> {
    syncer: Syncer<H, S, C>,
    receiver: mpsc::Receiver<CoreThreadCommand>,
}

enum CoreThreadCommand {
    AddBlocks(Vec<Data<StatementBlock>>, oneshot::Sender<()>),
    ForceNewBlock(RoundNumber, oneshot::Sender<()>),
    Cleanup(oneshot::Sender<()>),
}

impl<H: BlockHandler + 'static, S: SyncerSignals + 'static, C: CommitObserver + 'static>
    CoreThreadDispatcher<H, S, C>
{
    pub fn start(syncer: Syncer<H, S, C>) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let metrics = syncer.core().metrics.clone();
        let core_thread = CoreThread { syncer, receiver };
        let join_handle = thread::Builder::new()
            .name("mysticeti-core".to_string())
            .spawn(move || core_thread.run())
            .unwrap();
        #[cfg(unix)]
        tracing::info!(
            "Started core thread with pid {}",
            join_handle.as_pthread_t()
        );
        Self {
            sender,
            join_handle,
            metrics,
        }
    }

    pub fn stop(self) -> Syncer<H, S, C> {
        drop(self.sender);
        self.join_handle.join().unwrap()
    }

    pub async fn add_blocks(&self, blocks: Vec<Data<StatementBlock>>) {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::AddBlocks(blocks, sender))
            .await;
        receiver.await.expect("core thread is not expected to stop");
    }

    pub async fn force_new_block(&self, round: RoundNumber) {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::ForceNewBlock(round, sender))
            .await;
        receiver.await.expect("core thread is not expected to stop");
    }

    pub async fn cleanup(&self) {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::Cleanup(sender)).await;
        receiver.await.expect("core thread is not expected to stop");
    }

    async fn send(&self, command: CoreThreadCommand) {
        self.metrics.core_lock_enqueued.inc();
        if self.sender.send(command).await.is_err() {
            panic!("core thread is not expected to stop");
        }
    }
}

impl<H: BlockHandler, S: SyncerSignals, C: CommitObserver> CoreThread<H, S, C> {
    pub fn run(mut self) -> Syncer<H, S, C> {
        let metrics = self.syncer.core().metrics.clone();
        while let Some(command) = self.receiver.blocking_recv() {
            let _timer = metrics.core_lock_util.utilization_timer();
            metrics.core_lock_dequeued.inc();
            match command {
                CoreThreadCommand::AddBlocks(blocks, sender) => {
                    self.syncer.add_blocks(blocks);
                    sender.send(()).ok();
                }
                CoreThreadCommand::ForceNewBlock(round, sender) => {
                    self.syncer.force_new_block(round);
                    sender.send(()).ok();
                }
                CoreThreadCommand::Cleanup(sender) => {
                    self.syncer.core().cleanup();
                    sender.send(()).ok();
                }
            }
        }
        self.syncer
    }
}