use std::io;

use crate::runtime::agentic_reference::{
    AgentExecution, AgentRequest, AgentRuntime, BasicContextAgent, RuntimeResult,
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use crate::containers::connect_nats_with_retry;
use crate::debug::{debug_log, debug_log_value};
use crate::seed::bundle_event::{
    context_bundle_generated_subject, parse_context_bundle_generated_event,
};
use kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient;

pub struct EventDrivenRuntimeTrigger<R> {
    agent: BasicContextAgent<R>,
    nats_url: String,
    request_template: AgentRequest,
}

impl<R> EventDrivenRuntimeTrigger<R>
where
    R: AgentRuntime + Send + 'static,
{
    pub fn new(
        query_client: ContextQueryServiceClient<Channel>,
        runtime: R,
        nats_url: impl Into<String>,
        request_template: AgentRequest,
    ) -> Self {
        Self {
            agent: BasicContextAgent::new(query_client, runtime),
            nats_url: nats_url.into(),
            request_template,
        }
    }

    pub fn spawn(
        self,
    ) -> (
        JoinHandle<RuntimeResult<AgentExecution>>,
        oneshot::Receiver<()>,
    ) {
        let (ready_tx, ready_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let client = connect_nats_with_retry(&self.nats_url).await?;
            let subject = context_bundle_generated_subject();
            debug_log_value("subscribing bundle generated subject", &subject);
            let mut subscription = client.subscribe(subject).await?;
            let _ = ready_tx.send(());
            debug_log("bundle generated subscription ready");
            let message = subscription.next().await.ok_or_else(|| {
                io::Error::new(io::ErrorKind::UnexpectedEof, "bundle event stream closed")
            })?;
            debug_log("bundle generated event received");

            let event = parse_context_bundle_generated_event(message.payload.as_ref())?;
            debug_log_value("bundle generated root_node_id", &event.data.root_node_id);
            let mut request = self.request_template;
            request.root_node_id = event.data.root_node_id;
            if let Some(role) = event.data.roles.first() {
                request.role = role.clone();
            }

            let mut agent = self.agent;
            agent.execute(request).await
        });

        (handle, ready_rx)
    }
}
