# Choose a Bot model

When creating or editing a Bot in Dashboard, choose an Agent Client, then open **Model** to search its available models by name or ID. Switching clients clears the selected model. If loading fails, use **Retry**; if a saved model is no longer available, select another before saving.

Pi provides model discovery before a Bot or Session exists. Enable Pi in Settings and configure its provider credentials through Pi.

API consumers can send an authenticated `GET /api/v1/agent-clients/pi/default/models`. The general path is `/api/v1/agent-clients/{client_type}/{client_key}/models`, using the identity returned by the Agent Client listing. The client must be enabled and provide model discovery before Session creation.

The response's `data.models` is an array of objects containing `id`, `name`, and `description`. Use the exact `id` as the Bot's `model`; Pi IDs include the provider, such as `provider/model`. An empty array means no models are available. Query failures return an error rather than an empty catalog.
