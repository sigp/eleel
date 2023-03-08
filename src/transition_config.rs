use crate::types::{ErrorResponse, Request, Response, TransitionConfigurationV1};

pub async fn handle_transition_config(request: Request) -> Result<Response, ErrorResponse> {
    let (id, config) = request.parse_as::<(TransitionConfigurationV1,)>()?;
    // Tell the CL what it wants to hear.
    // TODO: actually check with the EL.
    Response::new(id, config)
}
