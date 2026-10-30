// SPDX-License-Identifier: LGPL-2.1-or-later
//! Peer-to-peer unix socket with fd passing, no bus daemon. No other socket is ever opened.

use crate::errors::BridgeError;
use crate::proxy::Bridge1Proxy;
use crate::signals::{decode_signal, Signal};
use crate::wire::Opts;
use futures::stream::{Stream, StreamExt};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::UnixStream;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub struct Connection {
    conn: zbus::Connection,
    proxy: Bridge1Proxy<'static>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Connection")
    }
}

pub async fn connect(socket_path: &Path) -> Result<Connection, BridgeError> {
    let stream = UnixStream::connect(socket_path).await?;
    Connection::from_stream(stream).await
}

impl Connection {
    /// Used by [`connect`] and the fake bridge.
    pub async fn from_stream(stream: UnixStream) -> Result<Connection, BridgeError> {
        let conn = zbus::connection::Builder::unix_stream(stream)
            .p2p()
            .build()
            .await?;
        let proxy = Bridge1Proxy::builder(&conn).build().await?;
        Ok(Connection { conn, proxy })
    }

    pub fn proxy(&self) -> &Bridge1Proxy<'static> {
        &self.proxy
    }

    pub fn zbus(&self) -> &zbus::Connection {
        &self.conn
    }

    pub async fn close(&self) {
        let _ = self.conn.clone().close().await;
    }

    /// Create before the first call or signals are missed.
    pub fn signals(&self) -> SignalStream {
        SignalStream {
            inner: zbus::MessageStream::from(&self.conn),
            done: false,
        }
    }

    /// Wipes `secret` before returning, also on failure or drop. The copy zbus serialises
    /// lives in the message buffer until sent.
    pub async fn connect_ad_hoc_wiping(
        &self,
        url: &str,
        secret: &mut Zeroizing<Vec<u8>>,
        opts: &Opts,
    ) -> Result<String, BridgeError> {
        let guard = WipeOnDrop(secret);
        let result = self.proxy.connect_ad_hoc(url, guard.0.as_slice(), opts).await;
        drop(guard);
        result.map_err(BridgeError::from)
    }

    pub async fn answer_wiping(
        &self,
        id: &str,
        accept: bool,
        answers: &mut Zeroizing<Vec<Vec<u8>>>,
    ) -> Result<(), BridgeError> {
        let mut builder = crate::wire::OptsBuilder::new().bool("accept", accept);
        if !answers.is_empty() {
            builder = builder.byte_arrays("answers", answers);
        }
        let opts = builder.build();
        let result = self.proxy.answer(id, &opts).await;
        for a in answers.iter_mut() {
            a.zeroize();
        }
        answers.clear();
        result.map_err(BridgeError::from)
    }
}

struct WipeOnDrop<'a>(&'a mut Zeroizing<Vec<u8>>);

impl Drop for WipeOnDrop<'_> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub struct SignalStream {
    inner: zbus::MessageStream,
    done: bool,
}

impl Stream for SignalStream {
    type Item = Result<Signal, BridgeError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if self.done {
                return Poll::Ready(None);
            }
            match self.inner.poll_next_unpin(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    self.done = true;
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Err(e))) => {
                    self.done = true;
                    return Poll::Ready(Some(Err(BridgeError::from(e))));
                }
                Poll::Ready(Some(Ok(msg))) => match decode_signal(&msg) {
                    Ok(Some(signal)) => return Poll::Ready(Some(Ok(signal))),
                    Ok(None) => continue,
                    Err(e) => return Poll::Ready(Some(Err(e))),
                },
            }
        }
    }
}
