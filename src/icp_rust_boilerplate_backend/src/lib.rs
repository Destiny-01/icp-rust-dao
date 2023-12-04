#[macro_use]
extern crate serde;
use candid::{Decode, Encode, Principal};
use ic_cdk::api::{time, caller};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Proposal {
    id: u64,
    title: String,
    details: String,
    amount_requested: u64,
    owner: Option<Principal>, 
    upvotes: Vec<Principal>,
    downvotes: Vec<Principal>,
    is_approved: bool,
    created_at: u64,
    deadline: u64,
    updated_at: Option<u64>,
}

// a trait that must be implemented for a struct that is stored in a stable struct
impl Storable for Proposal {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// another trait that must be implemented for a struct that is stored in a stable struct
impl BoundedStorable for Proposal {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static STORAGE: RefCell<StableBTreeMap<u64, Proposal, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct ProposalPayload {
    title: String,
    details: String,
    amount_requested: u64,
}

// Ability to get a single proposal
#[ic_cdk::query]
fn get_proposal(id: u64) -> Result<Proposal, Error> {
    match _get_proposal(&id) {
        Some(proposal) => Ok(proposal),
        None => Err(Error::NotFound {
            msg: format!("a proposal with id={} not found", id),
        }),
    }
}

// Ability to get all proposals
#[ic_cdk::query]
fn get_all_proposals() -> Result<Vec<Proposal>, Error> {
    let proposals : Vec<Proposal> =  STORAGE.with(|service| service.borrow().iter().map(|proposal| proposal.1).collect());
    let length = proposals.len();
    if length == 0 {
        return Err(Error::NotFound {
            msg: format!("No proposals found"),
        })
    }

    Ok(proposals)
}

// Ability to get all approved proposals that has ended
#[ic_cdk::query]
fn get_approved_proposals() -> Result<Vec<Proposal>, Error> {
    let proposal_map : Vec<(u64, Proposal)> =  STORAGE.with(|service| service.borrow().iter().collect());
    let length = proposal_map.len();
    if length == 0 {
        return Err(Error::NotFound {
            msg: format!("No proposals found"),
        })
    }

    let mut proposals: Vec<Proposal> = Vec::new();
    
    for key in 0..length {
        let proposal = proposal_map.get(key).unwrap().clone().1;
        if proposal.is_approved && _has_deadline_passed(proposal.deadline) {
            proposals.push(proposal);
        } else {
            continue;
        }
    }
 
    Ok(proposals)
}

// Ability to create a proposal that can be voted on within a week
#[ic_cdk::update]
fn add_proposal(proposal: ProposalPayload) -> Option<Proposal> {
    let upvotes: Vec<Principal> = Vec::new();
    let downvotes: Vec<Principal> = Vec::new();

    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");
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
        downvotes
    };
    do_insert(&proposal);
    Some(proposal)
}

// Ability to update a proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn update_proposal(id: u64, payload: ProposalPayload) -> Result<Proposal, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            if proposal.owner.is_some() && proposal.owner != Some(caller()) {
                return Err(Error::CantEditProposal {
                    msg: format!("Couldn't update proposal with id={}. You are not the owner", id),
                });
            }
            if _has_deadline_passed(proposal.deadline) {
                return Err(Error::DeadlineExceeded {
                    msg: format!("Couldn't vote on a proposal with id={}. Deadline exceeded", id),
                });
            }

            proposal.title = payload.title;
            proposal.details = payload.details;
            proposal.amount_requested = payload.amount_requested;
            proposal.updated_at = Some(time());
            do_insert(&proposal);
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

// Ability to update a proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn end_proposal_vote(id: u64) -> Result<Proposal, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            if proposal.owner.is_some() && proposal.owner != Some(caller()) {
                return Err(Error::CantEditProposal {
                    msg: format!("Couldn't update proposal with id={}. You are not the owner", id),
                });
            }
            if !_has_deadline_passed(proposal.deadline) {
                return Err(Error::DeadlineExceeded {
                    msg: format!("Voting period for proposal with id={} isn't over.", id),
                });
            }
            let total_votes = proposal.downvotes.len() - proposal.upvotes.len();
            if total_votes > 0 {
                proposal.is_approved = true;
            } else {
                proposal.is_approved = false;
            }
            do_insert(&proposal);
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
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            let can_vote = _check_if_can_vote(&proposal);
            if can_vote.is_err() {
                return Err(can_vote.unwrap_err());
            }
            proposal.upvotes.push(caller());
            do_insert(&proposal);
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
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut proposal) => {
            let can_vote = _check_if_can_vote(&proposal);
            if can_vote.is_err() {
                return Err(can_vote.unwrap_err());
            }
            proposal.downvotes.push(caller());
            do_insert(&proposal);
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

// Ability to delete proposal provided you're the owner and the deadline hasn't passed
#[ic_cdk::update]
fn delete_proposal(id: u64) -> Result<Proposal, Error> {
    let proposal = _get_proposal(&id).expect("Proposal not found");
    if proposal.owner.is_some() && proposal.owner != Some(caller()) {
        return Err(Error::CantEditProposal {
            msg: format!("Couldn't delete a proposal with id={}. You are not the owner", id),
        });
    }
    if _has_deadline_passed(proposal.deadline) {
        return Err(Error::DeadlineExceeded {
            msg: format!("Couldn't delete a proposal with id={}. Deadline exceeded", id),
        });
    }
    match STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(proposal) =>{
            Ok(proposal)
        },
        None => Err(Error::NotFound {
            msg: format!(
                "Couldn't delete a proposal with id={}. proposal not found.",
                id
            ),
        }),
    }
}

// Our error handling
#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
    HasVoted { msg: String },
    CantVoteYours { msg: String },
    CantEditProposal { msg: String },
    DeadlineExceeded { msg: String },
}

// helper method to perform insert.
fn do_insert(proposal: &Proposal) {
    STORAGE.with(|service| service.borrow_mut().insert(proposal.id, proposal.clone()));
}

// a helper method to get a proposal by id. used in get_proposal/update_proposal
fn _get_proposal(id: &u64) -> Option<Proposal> {
    STORAGE.with(|service| service.borrow().get(id))
}

// a helper method to check if a proposal deadline has passed
fn _has_deadline_passed(deadline: u64) -> bool {
    time() > deadline
}

fn _check_if_can_vote(proposal: &Proposal) -> Result<(), Error>{
    if proposal.owner.is_some() && proposal.owner == Some(caller()) {
        return Err(Error::CantVoteYours {
            msg: format!("Couldn't vote on a proposal with id={} because you created the proposal", proposal.id),
        });
    }

    let has_upvoted = proposal.upvotes.iter().position(|&user| user.to_string() == caller().to_string());
    if has_upvoted.is_some() {
        return Err(Error::HasVoted {
            msg: format!("Couldn't vote on a proposal with id={}. user voted already", proposal.id),
        });
    }
    let has_downvoted = proposal.downvotes.iter().position(|&user| user.to_string() == caller().to_string());
    if has_downvoted.is_some() {
        return Err(Error::HasVoted {
            msg: format!("Couldn't vote on a proposal with id={}. user voted already", proposal.id),
        });
    }

    if _has_deadline_passed(proposal.deadline) {
        return Err(Error::DeadlineExceeded {
            msg: format!("Couldn't vote on a proposal with id={}. Deadline exceeded", proposal.id),
        });
    }

    Ok(())
}

// need this to generate candid
ic_cdk::export_candid!();
