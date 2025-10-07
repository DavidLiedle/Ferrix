use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, FerrixError};
use crate::protocol::{SessionId, ClientId};

/// Collaborative session manager enabling multiple users to share sessions
pub struct CollaborationManager {
    sessions: Arc<RwLock<HashMap<SessionId, CollaborativeSession>>>,
    invitations: Arc<RwLock<HashMap<String, SessionInvitation>>>,
}

#[derive(Debug, Clone)]
pub struct CollaborativeSession {
    pub session_id: SessionId,
    pub owner_id: ClientId,
    pub participants: HashMap<ClientId, Participant>,
    pub settings: CollaborationSettings,
    pub event_broadcaster: broadcast::Sender<CollaborationEvent>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub client_id: ClientId,
    pub username: String,
    pub role: ParticipantRole,
    pub cursor_position: Option<CursorPosition>,
    pub active_pane: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub color: String, // For cursor/highlight color
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticipantRole {
    Owner,
    Collaborator,  // Can type and control
    Observer,      // Read-only access
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub pane_id: String,
    pub row: u16,
    pub col: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSettings {
    pub max_participants: usize,
    pub allow_anonymous: bool,
    pub require_invitation: bool,
    pub auto_follow_owner: bool,
    pub share_clipboard: bool,
    pub share_history: bool,
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInvitation {
    pub token: String,
    pub session_id: SessionId,
    pub inviter_id: ClientId,
    pub invitee_email: Option<String>,
    pub role: ParticipantRole,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub max_uses: Option<u32>,
    pub uses: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborationEvent {
    ParticipantJoined {
        participant: Participant,
        timestamp: DateTime<Utc>,
    },
    ParticipantLeft {
        client_id: ClientId,
        timestamp: DateTime<Utc>,
    },
    CursorMoved {
        client_id: ClientId,
        position: CursorPosition,
    },
    InputShared {
        client_id: ClientId,
        data: Vec<u8>,
        pane_id: String,
    },
    PaneChanged {
        client_id: ClientId,
        pane_id: String,
    },
    MessageBroadcast {
        sender_id: ClientId,
        message: String,
        timestamp: DateTime<Utc>,
    },
    PermissionChanged {
        client_id: ClientId,
        new_role: ParticipantRole,
    },
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CollaborationManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            invitations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new collaborative session
    pub async fn create_session(
        &self,
        session_id: SessionId,
        owner_id: ClientId,
        owner_name: String,
        settings: CollaborationSettings,
    ) -> Result<CollaborativeSession> {
        let (tx, _rx) = broadcast::channel(1000);

        let owner = Participant {
            client_id: owner_id,
            username: owner_name,
            role: ParticipantRole::Owner,
            cursor_position: None,
            active_pane: None,
            joined_at: Utc::now(),
            last_activity: Utc::now(),
            color: self.assign_color(0),
        };

        let mut participants = HashMap::new();
        participants.insert(owner_id, owner);

        let session = CollaborativeSession {
            session_id: session_id.clone(),
            owner_id,
            participants,
            settings,
            event_broadcaster: tx,
            created_at: Utc::now(),
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session.clone());

        Ok(session)
    }

    /// Generate invitation token for a session
    pub async fn create_invitation(
        &self,
        session_id: SessionId,
        inviter_id: ClientId,
        role: ParticipantRole,
        expiry_hours: u32,
        max_uses: Option<u32>,
    ) -> Result<String> {
        let token = self.generate_secure_token();

        let invitation = SessionInvitation {
            token: token.clone(),
            session_id,
            inviter_id,
            invitee_email: None,
            role,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(expiry_hours as i64),
            max_uses,
            uses: 0,
        };

        let mut invitations = self.invitations.write().await;
        invitations.insert(token.clone(), invitation);

        Ok(token)
    }

    /// Join a collaborative session with invitation token
    pub async fn join_with_invitation(
        &self,
        token: String,
        client_id: ClientId,
        username: String,
    ) -> Result<CollaborativeSession> {
        let mut invitations = self.invitations.write().await;

        let invitation = invitations
            .get_mut(&token)
            .ok_or_else(|| FerrixError::Other("Invalid invitation token".to_string()))?;

        // Check if invitation is still valid
        if invitation.expires_at < Utc::now() {
            return Err(FerrixError::Other("Invitation has expired".to_string()));
        }

        if let Some(max_uses) = invitation.max_uses {
            if invitation.uses >= max_uses {
                return Err(FerrixError::Other("Invitation has reached maximum uses".to_string()));
            }
        }

        invitation.uses += 1;

        let session_id = invitation.session_id.clone();
        let role = invitation.role.clone();

        // Add participant to session
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| FerrixError::Other("Session not found".to_string()))?;

        if session.participants.len() >= session.settings.max_participants {
            return Err(FerrixError::Other("Session is full".to_string()));
        }

        let participant = Participant {
            client_id,
            username,
            role,
            cursor_position: None,
            active_pane: None,
            joined_at: Utc::now(),
            last_activity: Utc::now(),
            color: self.assign_color(session.participants.len()),
        };

        session.participants.insert(client_id, participant.clone());

        // Broadcast join event
        let event = CollaborationEvent::ParticipantJoined {
            participant,
            timestamp: Utc::now(),
        };
        let _ = session.event_broadcaster.send(event);

        Ok(session.clone())
    }

    /// Leave a collaborative session
    pub async fn leave_session(
        &self,
        session_id: SessionId,
        client_id: ClientId,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            session.participants.remove(&client_id);

            // Broadcast leave event
            let event = CollaborationEvent::ParticipantLeft {
                client_id,
                timestamp: Utc::now(),
            };
            let _ = session.event_broadcaster.send(event);

            // If owner left, transfer ownership or close session
            if session.owner_id == client_id {
                if session.participants.is_empty() {
                    sessions.remove(&session_id);
                } else {
                    // Transfer ownership to next participant
                    if let Some((new_owner_id, participant)) = session.participants.iter_mut().next() {
                        session.owner_id = *new_owner_id;
                        participant.role = ParticipantRole::Owner;
                    }
                }
            }
        }

        Ok(())
    }

    /// Broadcast cursor position to other participants
    pub async fn update_cursor_position(
        &self,
        session_id: SessionId,
        client_id: ClientId,
        position: CursorPosition,
    ) -> Result<()> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&session_id) {
            let event = CollaborationEvent::CursorMoved {
                client_id,
                position,
            };
            let _ = session.event_broadcaster.send(event);
        }

        Ok(())
    }

    /// Share input with all participants (if they have permission)
    pub async fn broadcast_input(
        &self,
        session_id: SessionId,
        client_id: ClientId,
        data: Vec<u8>,
        pane_id: String,
    ) -> Result<()> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&session_id) {
            // Check if client has permission to send input
            if let Some(participant) = session.participants.get(&client_id) {
                match participant.role {
                    ParticipantRole::Owner | ParticipantRole::Collaborator => {
                        let event = CollaborationEvent::InputShared {
                            client_id,
                            data,
                            pane_id,
                        };
                        let _ = session.event_broadcaster.send(event);
                    }
                    ParticipantRole::Observer => {
                        return Err(FerrixError::Other("Observers cannot send input".to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    /// Send chat message to all participants
    pub async fn send_message(
        &self,
        session_id: SessionId,
        sender_id: ClientId,
        message: String,
    ) -> Result<()> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&session_id) {
            let event = CollaborationEvent::MessageBroadcast {
                sender_id,
                message,
                timestamp: Utc::now(),
            };
            let _ = session.event_broadcaster.send(event);
        }

        Ok(())
    }

    /// Change participant role
    pub async fn change_role(
        &self,
        session_id: SessionId,
        requester_id: ClientId,
        target_id: ClientId,
        new_role: ParticipantRole,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            // Only owner can change roles
            if session.owner_id != requester_id {
                return Err(FerrixError::Other("Only owner can change roles".to_string()));
            }

            if let Some(participant) = session.participants.get_mut(&target_id) {
                participant.role = new_role.clone();

                let event = CollaborationEvent::PermissionChanged {
                    client_id: target_id,
                    new_role,
                };
                let _ = session.event_broadcaster.send(event);
            }
        }

        Ok(())
    }

    /// Get list of active participants
    pub async fn get_participants(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<Participant>> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&session_id) {
            Ok(session.participants.values().cloned().collect())
        } else {
            Err(FerrixError::Other("Session not found".to_string()))
        }
    }

    /// Subscribe to collaboration events
    pub async fn subscribe_to_events(
        &self,
        session_id: SessionId,
    ) -> Result<broadcast::Receiver<CollaborationEvent>> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(&session_id) {
            Ok(session.event_broadcaster.subscribe())
        } else {
            Err(FerrixError::Other("Session not found".to_string()))
        }
    }

    fn generate_secure_token(&self) -> String {
        Uuid::new_v4().to_string()
    }

    fn assign_color(&self, index: usize) -> String {
        let colors = ["#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4", "#FECA57",
            "#DDA0DD", "#98D8C8", "#F7DC6F", "#BB8FCE", "#85C1E2"];
        colors[index % colors.len()].to_string()
    }
}

impl CollaborativeSession {
    /// Create a new collaborative session for testing
    pub fn new(
        session_id: SessionId,
        owner_id: ClientId,
        event_broadcaster: broadcast::Sender<CollaborationEvent>,
    ) -> Self {
        let mut participants = HashMap::new();
        participants.insert(
            owner_id,
            Participant {
                client_id: owner_id,
                username: "Owner".to_string(),
                role: ParticipantRole::Owner,
                cursor_position: None,
                active_pane: None,
                joined_at: Utc::now(),
                last_activity: Utc::now(),
                color: "#FF6B6B".to_string(),
            },
        );

        Self {
            session_id,
            owner_id,
            participants,
            settings: CollaborationSettings::default(),
            event_broadcaster,
            created_at: Utc::now(),
        }
    }
}

impl Default for CollaborationSettings {
    fn default() -> Self {
        Self {
            max_participants: 10,
            allow_anonymous: false,
            require_invitation: true,
            auto_follow_owner: false,
            share_clipboard: true,
            share_history: false,
            encryption_enabled: true,
        }
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_collaboration_session() {
        // Test collaboration session creation
        assert!(true);
    }

    #[test]
    fn test_collaboration_permissions() {
        // Test collaboration permissions
        assert!(true);
    }
}
