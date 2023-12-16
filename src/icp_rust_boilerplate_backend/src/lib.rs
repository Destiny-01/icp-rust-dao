#[macro_use]
extern crate serde;
use candid::{Decode, Encode, Principal};
use ic_cdk::api::{caller, time};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

// Define custom types for memory and id cell
type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

// Define structs for Proposal, Dao, and Comment
#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Proposal {
    id: u64,
    dao_id: u64,
    title: String,
    details: String,
    amount_requested: u64,
    owner: Option<Principal>,
    upvotes: Vec<Principal>,
    downvotes: Vec<Principal>,
    is_approved: bool,
    created_at: u64,
    comments: Vec<u64>,
    deadline: u64,
    updated_at: Option<u64>,
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Dao {
    id: u64,
    name: String,
    description: String,
    avatar: String,
    owner: Option<Principal>,
    members: Vec<Principal>,
    proposals: Vec<u64>,
    created_at: u64,
    updated_at: Option<u64>,
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Comment {
    id: u64,
    content: String,
    author: Option<Principal>,
    likes: Vec<Principal>,
    proposal_id: u64,
    created_at: u64,
    updated_at: Option<u64>,
}

// Implement Storable trait for Proposal, Dao, and Comment
impl Storable for Proposal {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl Storable for Dao {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl Storable for Comment {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// Implement BoundedStorable trait for Proposal, Dao, and Comment
impl BoundedStorable for Proposal {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

impl BoundedStorable for Dao {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

impl BoundedStorable for Comment {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

// Thread-local storage for memory manager, id counter, proposal storage, dao storage, and comment storage
thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static PROPOSAL_STORAGE: RefCell<StableBTreeMap<u64, Proposal, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));

    static DAO_STORAGE: RefCell<StableBTreeMap<u64, Dao, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(2)))
    ));

    static COMMENT_STORAGE: RefCell<StableBTreeMap<u64, Comment, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(3)))
    ));
}

// Structs for payload data (ProposalPayload, DaoPayload, CommentPayload)
#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct ProposalPayload {
    title: String,
    details: String,
    amount_requested: u64,
    dao_id: u64,
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct DaoPayload {
    name: String,
    description: String,
    avatar: String,
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct CommentPayload {
    content: String,
    proposal_id: u64,
}

/**
 * -----------------------------------------------------------------------------
 * DAO RELATED FUNCTIONS
 * -----------------------------------------------------------------------------
**/
// Ability to get DAOs user is part of
#[ic_cdk::query]
fn get_user_daos() -> Result<Vec<Dao>, Error> {
    let daos_map: Vec<(u64, Dao)> = DAO_STORAGE.with(|service| service.borrow().iter().collect());
    if daos_map.is_empty() {
        return Err(Error::NotFound {
            msg: format!("No dao found. Why don't you try joining or creating one"),
        });
    }

    let all_daos: Vec<Dao> = daos_map.into_iter().map(|(_, dao)| dao).collect();

    let user_daos: Vec<Dao> = all_daos
        .iter()
        .filter(|dao| dao.owner == Some(caller()) || dao.members.contains(&caller()))
        .cloned()
        .collect();

    Ok(user_daos)
}

// Ability to get a single DAO
#[ic_cdk::query]
fn get_dao(id: u64) -> Result<Dao, Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&id);

    match is_user_part_of_dao {
        Some(_is_true) => match _get_dao(&id) {
            Some(dao) => Ok(dao),
            None => Err(Error::NotFound {
                msg: format!("a dao with id={} not found", id),
            }),
        },
        None => Err(Error::NotAMember {
            msg: format!("unable to get a dao with id={}. Not a member", id),
        }),
    }
}

// Ability to create a DAO
#[ic_cdk::update]
fn create_dao(dao: DaoPayload) -> Option<Dao> {
    let members: Vec<Principal> = Vec::new();
    let proposals: Vec<u64> = Vec::new();

    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");
    let dao = Dao {
        id,
        name: dao.name,
        description: dao.description,
        avatar: dao.avatar,
        owner: Some(caller()),
        created_at: time(),
        updated_at: None,
        members,
        proposals,
    };

    do_insert_dao(&dao);
    Some(dao)
}

// Ability to update a DAO providing you're the owner
#[ic_cdk::update]
fn update_dao(id: u64, payload: DaoPayload) -> Result<Dao, Error> {
    match DAO_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut dao) => {
            if dao.owner.is_some() && dao.owner != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!("Couldn't update dao with id={}. You are not the owner", id),
                });
            }

            dao.name = payload.name;
            dao.description = payload.description;
            dao.avatar = payload.avatar;
            dao.updated_at = Some(time());

            do_insert_dao(&dao);
            Ok(dao)
        }
        None => Err(Error::NotFound {
            msg: format!("couldn't update a dao with id={}. dao not found", id),
        }),
    }
}

// Ability to delete DAO provided you're the owner
#[ic_cdk::update]
fn delete_dao(id: u64) -> Result<Dao, Error> {
    match DAO_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(dao) => {
            if dao.owner.is_some() && dao.owner != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!(
                        "Couldn't delete a dao with id={}. You are not the owner",
                        id
                    ),
                });
            }

            dao.proposals.iter().for_each(|proposal_id| {
                PROPOSAL_STORAGE.with(|service| service.borrow_mut().remove(&proposal_id));
            });

            Ok(dao)
        }
        None => Err(Error::NotFound {
            msg: format!("Couldn't delete a dao with id={}. dao not found.", id),
        }),
    }
}

/**
* -----------------------------------------------------------------------------
* PROPOSAL FUNCTIONS (callable if user is part of DAO)
* -----------------------------------------------------------------------------
*/

// Ability to get a single proposal
#[ic_cdk::query]
fn get_proposal(id: u64) -> Result<Proposal, Error> {
    match _get_proposal(&id) {
        Some(proposal) => {
            let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&proposal.dao_id);
            match is_user_part_of_dao {
                Some(_is_true) => Ok(proposal),
                None => Err(Error::NotAMember {
                    msg: format!("unable to get a dao with id={}. Not a member", id),
                }),
            }
        }
        None => Err(Error::NotFound {
            msg: format!("a proposal with id={} not found", id),
        }),
    }
}

// Ability to get all proposals in the DAO
#[ic_cdk::query]
fn get_all_proposals(dao_id: u64) -> Result<Vec<Proposal>, Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&dao_id);
    match is_user_part_of_dao {
        Some(_is_true) => {
            let proposals_map: Vec<(u64, Proposal)> =
                PROPOSAL_STORAGE.with(|service| service.borrow().iter().collect());
            let length = proposals_map.len();
            if length == 0 {
                return Err(Error::NotFound {
                    msg: format!("No proposals found"),
                });
            }

            let mut proposals: Vec<Proposal> = Vec::new();

            for key in 0..length {
                let proposal = proposals_map.get(key).unwrap().clone().1;
                if proposal.dao_id == dao_id {
                    proposals.push(proposal);
                } else {
                    continue;
                }
            }

            Ok(proposals)
        }
        None => Err(Error::NotAMember {
            msg: format!("unable to get a dao with id={}. Not a member", dao_id),
        }),
    }
}

// Ability to get all approved proposals that has ended
#[ic_cdk::query]
fn get_final_approved_proposals(dao_id: u64) -> Result<Vec<Proposal>, Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&dao_id);
    match is_user_part_of_dao {
        Some(_is_true) => {
            let proposals_map: Vec<(u64, Proposal)> =
                PROPOSAL_STORAGE.with(|service| service.borrow().iter().collect());
            let length = proposals_map.len();
            if length == 0 {
                return Err(Error::NotFound {
                    msg: format!("No proposals found"),
                });
            }

            let mut proposals: Vec<Proposal> = Vec::new();

            for key in 0..length {
                let proposal = proposals_map.get(key).unwrap().clone().1;
                if proposal.is_approved
                    && proposal.dao_id == dao_id
                    && is_deadline_not_reaached(proposal.deadline)
                {
                    proposals.push(proposal);
                } else {
                    continue;
                }
            }

            Ok(proposals)
        }
        None => Err(Error::NotAMember {
            msg: format!("unable to get a dao with id={}. Not a member", dao_id),
        }),
    }
}

// Ability to create a proposal that can be voted on within a week
#[ic_cdk::update]
fn add_proposal(proposal: ProposalPayload) -> Result<Proposal, Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&proposal.dao_id);
    match is_user_part_of_dao {
        Some(_is_true) => {
            let upvotes: Vec<Principal> = Vec::new();
            let downvotes: Vec<Principal> = Vec::new();
            let comments: Vec<u64> = Vec::new();

            let id = ID_COUNTER
                .with(|counter| {
                    let current_value = *counter.borrow().get();
                    counter.borrow_mut().set(current_value + 1)
                })
                .expect("cannot increment id counter");

            match DAO_STORAGE.with(|service| service.borrow().get(&proposal.dao_id)) {
                Some(mut dao) => {
                    dao.proposals.push(id);
                    dao.updated_at = Some(time());

                    do_insert_dao(&dao);
                }
                None => (),
            }

            let proposal = Proposal {
                id,
                title: proposal.title,
                details: proposal.details,
                amount_requested: proposal.amount_requested,
                owner: Some(caller()),
                created_at: time(),
                deadline: time() + (7 * 24 * 60 * 60 * 1_000_000_000), // one week
                updated_at: None,
                upvotes,
                is_approved: false,
                dao_id: proposal.dao_id,
                comments,
                downvotes,
            };

            do_insert_proposal(&proposal);
            Ok(proposal)
        }
        None => Err(Error::NotAMember {
            msg: format!(
                "unable to get a dao with id={}. Not a member",
                proposal.dao_id
            ),
        }),
    }
}

// Ability to update a proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn update_proposal(id: u64, payload: ProposalPayload) -> Result<Proposal, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            if proposal.owner.is_some() && proposal.owner != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!(
                        "Couldn't update proposal with id={}. You are not the owner",
                        id
                    ),
                });
            }
            if is_deadline_not_reaached(proposal.deadline) {
                return Err(Error::DeadlineExceeded {
                    msg: format!(
                        "Couldn't vote on a proposal with id={}. Deadline exceeded",
                        id
                    ),
                });
            }

            proposal.title = payload.title;
            proposal.details = payload.details;
            proposal.amount_requested = payload.amount_requested;
            proposal.updated_at = Some(time());

            do_insert_proposal(&proposal);
            Ok(proposal)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update a proposal with id={}. proposal not found",
                id
            ),
        }),
    }
}

// Ability to upvote a proposal provided you're not the owner, you haven't voted and the deadline hasn't passed
#[ic_cdk::update]
fn upvote(id: u64) -> Result<Proposal, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            let can_vote = _check_if_can_vote(&proposal, &proposal.dao_id);
            if can_vote.is_err() {
                return Err(can_vote.unwrap_err());
            }
            proposal.upvotes.push(caller());

            do_insert_proposal(&proposal);
            Ok(proposal)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't vote on a proposal with id={}. proposal not found",
                id
            ),
        }),
    }
}

// Ability to downvote a proposal provided you're not the owner, you haven't voted and the deadline hasn't passed
#[ic_cdk::update]
fn downvote(id: u64) -> Result<Proposal, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            let can_vote = _check_if_can_vote(&proposal, &proposal.dao_id);
            if can_vote.is_err() {
                return Err(can_vote.unwrap_err());
            }
            proposal.downvotes.push(caller());

            do_insert_proposal(&proposal);
            Ok(proposal)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't vote on a proposal with id={}. proposal not found",
                id
            ),
        }),
    }
}

// Ability to end a proposal provided you're the owner and the deadline has passed
#[ic_cdk::update]
fn end_proposal_vote(id: u64) -> Result<Proposal, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            if proposal.owner.is_some() && proposal.owner != Some(caller()) {
                return Err(Error::CantEditProposal {
                    msg: format!(
                        "Couldn't update proposal with id={}. You are not the owner",
                        id
                    ),
                });
            }
            if !is_deadline_not_reaached(proposal.deadline) {
                return Err(Error::DeadlineNotExceeded {
                    msg: format!("Voting period for proposal with id={} isn't over.", id),
                });
            }

            let total_votes = proposal.downvotes.len() - proposal.upvotes.len();
            if total_votes > 0 {
                proposal.is_approved = true;
            } else {
                proposal.is_approved = false;
            }

            do_insert_proposal(&proposal);
            Ok(proposal)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update a proposal with id={}. proposal not found",
                id
            ),
        }),
    }
}

// Ability to delete proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn delete_proposal(id: u64) -> Result<Proposal, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(proposal) => {
            if proposal.owner.is_some() && proposal.owner != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!(
                        "Couldn't delete a proposal with id={}. You are not the owner",
                        id
                    ),
                });
            }
            if is_deadline_not_reaached(proposal.deadline) {
                return Err(Error::DeadlineExceeded {
                    msg: format!(
                        "Couldn't delete a proposal with id={}. Deadline exceeded",
                        id
                    ),
                });
            }

            match DAO_STORAGE.with(|service| service.borrow().get(&proposal.dao_id)) {
                Some(mut dao) => {
                    dao.proposals.retain(|x| *x != id);

                    do_insert_dao(&dao);
                }
                None => {}
            }

            proposal.comments.iter().for_each(|comment_id| {
                COMMENT_STORAGE.with(|service| service.borrow_mut().remove(&comment_id));
            });

            Ok(proposal)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "Couldn't delete a proposal with id={}. proposal not found.",
                id
            ),
        }),
    }
}

/**
* -----------------------------------------------------------------------------
* COMMENT FUNCTIONS
* -----------------------------------------------------------------------------
*/

// Ability to get all comments on a proposal
#[ic_cdk::query]
fn get_all_comments_on_proposal(proposal_id: u64, dao_id: u64) -> Result<Vec<Comment>, Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&dao_id);
    match is_user_part_of_dao {
        Some(_is_true) => {
            let comments_map: Vec<(u64, Comment)> =
                COMMENT_STORAGE.with(|service| service.borrow().iter().collect());
            if comments_map.is_empty() {
                return Err(Error::NotFound {
                    msg: format!("No comments found. Why don't you try creating one"),
                });
            }

            let proposal_comments: Vec<Comment> = comments_map
                .into_iter()
                .filter(|(_, comment)| comment.proposal_id == proposal_id)
                .map(|(_, comment)| comment)
                .collect();

            Ok(proposal_comments)
        }
        None => Err(Error::NotAMember {
            msg: format!("unable to get a dao with id={}. Not a member", dao_id),
        }),
    }
}

// Ability to comment a proposal that can be voted on within a week
#[ic_cdk::update]
fn comment_on_post(comment: CommentPayload) -> Result<Comment, Error> {
    match PROPOSAL_STORAGE.with(|service| service.borrow().get(&comment.proposal_id)) {
        Some(mut proposal) => {
            let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&proposal.dao_id);
            match is_user_part_of_dao {
                Some(_is_true) => {
                    let likes: Vec<Principal> = Vec::new();

                    let id = ID_COUNTER
                        .with(|counter| {
                            let current_value = *counter.borrow().get();
                            counter.borrow_mut().set(current_value + 1)
                        })
                        .expect("cannot increment id counter");

                    proposal.comments.push(id);
                    proposal.updated_at = Some(time());

                    do_insert_proposal(&proposal);

                    let comment = Comment {
                        id,
                        content: comment.content,
                        proposal_id: comment.proposal_id,
                        created_at: time(),
                        updated_at: None,
                        likes,
                        author: Some(caller()),
                    };

                    do_insert_comment(&comment);
                    Ok(comment)
                }
                None => Err(Error::NotAMember {
                    msg: format!(
                        "unable to get a dao with id={}. Not a member",
                        proposal.dao_id
                    ),
                }),
            }
        }
        None => Err(Error::NotAMember {
            msg: format!(
                "cannot comment on proposal with id={}. Not found",
                comment.proposal_id
            ),
        }),
    }
}

// Ability to update a proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn update_comment(id: u64, payload: CommentPayload) -> Result<Comment, Error> {
    match COMMENT_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut comment) => {
            if comment.author.is_some() && comment.author != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!(
                        "Couldn't update comment with id={}. You are not the owner",
                        id
                    ),
                });
            }

            comment.content = payload.content;
            comment.updated_at = Some(time());

            do_insert_comment(&comment);
            Ok(comment)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update a comment with id={}. comment not found",
                id
            ),
        }),
    }
}

// Ability to like a coment provided you're not the owner and you haven't liked
#[ic_cdk::update]
fn like_comment(id: u64, dao_id: u64) -> Result<Comment, Error> {
    match COMMENT_STORAGE.with(|service| service.borrow_mut().get(&id)) {
        Some(mut comment) => match _is_user_part_of_dao(&dao_id) {
            Some(_is_true) => {
                if comment.author.is_some() && comment.author == Some(caller()) {
                    return Err(Error::CantLikeYours {
                        msg: format!(
                            "Couldn't like a comment with id={} because you created the comment",
                            comment.id
                        ),
                    });
                }

                let has_liked = comment.likes.iter().any(|user| *user == caller());
                if has_liked {
                    return Err(Error::HasVoted {
                        msg: format!(
                            "Couldn't like a comment with id={}. User has already liked",
                            comment.id
                        ),
                    });
                }

                comment.likes.push(caller());

                do_insert_comment(&comment);
                Ok(comment)
            }
            None => Err(Error::NotFound {
                msg: format!("Dao of id={} not found.", dao_id),
            }),
        },
        None => Err(Error::NotFound {
            msg: format!(
                "Couldn't vote on a comment with id={}. Comment not found",
                id
            ),
        }),
    }
}

// Ability to delete proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn delete_comment(id: u64) -> Result<Comment, Error> {
    match COMMENT_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(comment) => {
            if comment.author.is_some() && comment.author != Some(caller()) {
                return Err(Error::PermissionError {
                    msg: format!(
                        "Couldn't delete a comment with id={}. You are not the owner",
                        id
                    ),
                });
            }

            match PROPOSAL_STORAGE.with(|service| service.borrow().get(&comment.proposal_id)) {
                Some(mut proposal) => {
                    proposal.comments.retain(|x| *x != id);

                    do_insert_proposal(&proposal);
                }
                None => {}
            }

            Ok(comment)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "Couldn't delete a comment with id={}. comment not found.",
                id
            ),
        }),
    }
}

/**
* -----------------------------------------------------------------------------
* ERRORS
* -----------------------------------------------------------------------------
*/

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
    NotAMember { msg: String },
    HasVoted { msg: String },
    CantVoteYours { msg: String },
    CantLikeYours { msg: String },
    CantEditProposal { msg: String },
    PermissionError { msg: String },
    DeadlineExceeded { msg: String },
    DeadlineNotExceeded { msg: String },
}

/**
* -----------------------------------------------------------------------------
* HELPER FUNCTIONS
* -----------------------------------------------------------------------------
*/

// helper method to perform insert.
fn do_insert_proposal(proposal: &Proposal) {
    PROPOSAL_STORAGE.with(|service| service.borrow_mut().insert(proposal.id, proposal.clone()));
}

// helper method to perform insert.
fn do_insert_dao(dao: &Dao) {
    DAO_STORAGE.with(|service| service.borrow_mut().insert(dao.id, dao.clone()));
}

// helper method to perform insert.
fn do_insert_comment(comment: &Comment) {
    COMMENT_STORAGE.with(|service| service.borrow_mut().insert(comment.id, comment.clone()));
}

// a helper method to get a proposal by id. used in get_proposal/update_proposal
fn _get_proposal(id: &u64) -> Option<Proposal> {
    PROPOSAL_STORAGE.with(|service| service.borrow().get(id))
}

fn _get_dao(id: &u64) -> Option<Dao> {
    DAO_STORAGE.with(|service| service.borrow().get(id))
}

fn _get_comment(id: &u64) -> Option<Comment> {
    COMMENT_STORAGE.with(|service| service.borrow().get(id))
}

// a helper method to check if a proposal deadline has passed
fn is_deadline_not_reaached(deadline: u64) -> bool {
    time() > deadline
}

// Check if a user is eligible to vote
fn _check_if_can_vote(proposal: &Proposal, id: &u64) -> Result<(), Error> {
    let is_user_part_of_dao: Option<bool> = _is_user_part_of_dao(&id);
    match is_user_part_of_dao {
        Some(_is_true) => {
            if proposal.owner.is_some() && proposal.owner == Some(caller()) {
                return Err(Error::CantVoteYours {
                    msg: format!(
                        "Couldn't vote on a proposal with id={} because you created the proposal",
                        proposal.id
                    ),
                });
            }

            let has_upvoted = proposal
                .upvotes
                .iter()
                .position(|&user| user.to_string() == caller().to_string());
            if has_upvoted.is_some() {
                return Err(Error::HasVoted {
                    msg: format!(
                        "Couldn't vote on a proposal with id={}. user voted already",
                        proposal.id
                    ),
                });
            }
            let has_downvoted = proposal
                .downvotes
                .iter()
                .position(|&user| user.to_string() == caller().to_string());
            if has_downvoted.is_some() {
                return Err(Error::HasVoted {
                    msg: format!(
                        "Couldn't vote on a proposal with id={}. user voted already",
                        proposal.id
                    ),
                });
            }

            if is_deadline_not_reaached(proposal.deadline) {
                return Err(Error::DeadlineExceeded {
                    msg: format!(
                        "Couldn't vote on a proposal with id={}. Deadline exceeded",
                        proposal.id
                    ),
                });
            }

            Ok(())
        }
        None => Err(Error::NotFound {
            msg: format!("Dao of id={} not found.", id),
        }),
    }
}

// Check if a user is part of a DAO
fn _is_user_part_of_dao(id: &u64) -> Option<bool> {
    match _get_dao(&id) {
        Some(dao) => {
            let is_part = dao.owner == Some(caller()) || dao.members.contains(&caller());

            if is_part {
                return Some(true);
            }
            return None;
        }
        None => return None,
    }
}

// need this to generate candid
ic_cdk::export_candid!();
