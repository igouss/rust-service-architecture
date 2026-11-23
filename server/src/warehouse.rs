use domain::fulfilment::{FulfilmentReply, FulfilmentRequest};
use ports::{Gateway, GatewayError, RequestId, StreamId};

pub struct AlwaysInStock;

impl Gateway<FulfilmentRequest> for AlwaysInStock {
    type Reply = FulfilmentReply;

    /// When stock is to be reserved, `perform` shall answer that it is reserved.
    /// When an order is to be shipped, `perform` shall answer that it shipped.
    async fn perform(&self, id: RequestId, stream: &StreamId, request: &FulfilmentRequest) -> Result<FulfilmentReply, GatewayError> {
        tracing::info!(request = id.get(), fulfilment = %stream.0, "the demonstration warehouse performs {request:?}");
        match request {
            FulfilmentRequest::ReserveStock => Ok(FulfilmentReply::StockReserved),
            FulfilmentRequest::Ship => Ok(FulfilmentReply::Shipped),
        }
    }
}
