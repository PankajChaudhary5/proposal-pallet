#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

use frame_support::{
	codec::{Decode, Encode},
	dispatch::DispatchResult,
	inherent::Vec,
	sp_runtime::RuntimeDebug,
	traits::{Currency, ExistenceRequirement},
};
use scale_info::TypeInfo;

pub type MemberCount = u32;
pub type ProposalId<T> = <T as frame_system::Config>::Hash;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Votes<AccountId> {
	ayes: Vec<AccountId>,
	nays: Vec<AccountId>,
}

#[derive(PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ProposalInfo<Balance> {
	title: Vec<u8>,
	amount: Balance,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum Vote {
	Aye,
	Nay,
}

#[frame_support::pallet]
pub mod pallet {
	use crate::{MemberCount, ProposalId, ProposalInfo, Vote, Votes};
	use frame_support::{
		inherent::Vec,
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, ReservableCurrency},
	};
	use frame_system::pallet_prelude::*;
	use scale_info::prelude::vec;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	pub type BalanceIn<Runtime> = <<Runtime as Config>::Currency as Currency<
		<Runtime as frame_system::Config>::AccountId,
	>>::Balance;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type Currency: ReservableCurrency<Self::AccountId>;

		type TimeDuration: Get<u32>;
	}

	#[pallet::storage]
	#[pallet::getter(fn transfer_time)]
	pub type TransferTime<T: Config> =
		StorageMap<_, Blake2_128Concat, BlockNumberFor<T>, ProposalId<T>, ValueQuery>;

	/// Accounts which raised proposal to take funds
	#[pallet::storage]
	#[pallet::getter(fn fund_seeker_accounts)]
	pub type FundSeekerAccounts<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, Vec<T::AccountId>, ValueQuery>;

	/// list of community members,Anyone can join in this list
	#[pallet::storage]
	#[pallet::getter(fn community_members)]
	pub type CommunityMembers<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	/// Members from community can join in committee list, action should be perform from sudo.
	#[pallet::storage]
	#[pallet::getter(fn committee_members)]
	pub type CommitteeMembers<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	/// Count the approval for any particular proposal.
	#[pallet::storage]
	#[pallet::getter(fn voting)]
	pub type Voting<T: Config> = StorageMap<_, Identity, T::Hash, Votes<T::AccountId>, OptionQuery>;

	/// Stores the proposal propose by any members.
	#[pallet::storage]
	#[pallet::getter(fn proposal)]
	pub type Proposal<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, ProposalInfo<BalanceIn<T>>, OptionQuery>;

	/// List of Approver's which approve any proposal. Only Committee members are allowed to
	/// approve.
	#[pallet::storage]
	#[pallet::getter(fn approvers)]
	pub type Approvers<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, Vec<T::AccountId>, ValueQuery>;

	/// Account from where funds will be transfer.
	#[pallet::storage]
	#[pallet::getter(fn pot_account)]
	pub type PotAccount<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	/// Pallets use events to inform users when important changes are made.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MemberAdded,
		MemberAddedToCommittee,
		ProposalReject,
		ProposalApproved,
		ProposalAdded,
		FundTransfer,
		FundTransferDeclined,
		Approved {
			account: T::AccountId,
			proposal_hash: T::Hash,
			voted: Vote,
			ayes: MemberCount,
			nays: MemberCount,
		},
	}

	/// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// If a member try to add in a community for multiple times.
		AlreadyMemberOfCommunity,
		/// If sudo try to add a community member in a committee multiple times.
		AlreadyMemberOfCommittee,
		/// If sudo try to add a member which is not a part of community.
		MemberIsNotPresentInCommunity,
		/// If a member try to add the same proposal multiple time.
		ProposalAlreadyExist,
		/// If a member try to approve the proposal and not a member committee.
		MemberIsNotPresentInCommittee,
		/// If a committee member try to approve a same proposal multiple times.
		AlreadyApproved,
		/// If a member try to approve the wrong proposal.
		ProposalMissing,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let transaction_time = TransferTime::<T>::contains_key(n);
			// Time after which action will take place according to votes.
			if transaction_time {
				let proposal_id = TransferTime::<T>::get(n);
				let _result = Pallet::<T>::transfer_funds(proposal_id);
			}
			Weight::zero()
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Anyone can join in the community member's list.
		#[pallet::weight(10_000)]
		pub fn add_community_member(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			ensure_signed(origin.clone())?;

			// fetch the existing members from the community list.
			let mut members = CommunityMembers::<T>::get();
			// Search the new member in the existing member's list.
			let location =
				members.binary_search(&who).err().ok_or(Error::<T>::AlreadyMemberOfCommunity)?;

			// add the new member in the community member's list.
			members.insert(location, who.clone());

			CommunityMembers::<T>::put(&members);

			Self::deposit_event(Event::MemberAdded);
			Ok(())
		}

		/// Add member's from community from committee list
		/// Only Sudo can perform this action.
		#[pallet::weight(10_000)]
		pub fn add_committee_member(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			// Check origin is root.
			ensure_root(origin.clone())?;

			// member should be present in community members list
			let community_member = CommunityMembers::<T>::get();
			let _is_present = community_member
				.binary_search(&who)
				.map_err(|_| Error::<T>::MemberIsNotPresentInCommunity)?;

			let mut members = CommitteeMembers::<T>::get();
			let location =
				members.binary_search(&who).err().ok_or(Error::<T>::AlreadyMemberOfCommittee)?;

			members.insert(location, who.clone());

			// Add member into the committee member's list
			CommitteeMembers::<T>::put(&members);

			Self::deposit_event(Event::MemberAddedToCommittee);
			Ok(())
		}

		/// Propose the Proposal to take funds
		/// Anyone from community member's can propose.
		#[pallet::weight(10_000_000)]
		pub fn add_proposal(
			origin: OriginFor<T>,
			title: Vec<u8>,
			proposal_hash: T::Hash,
			amount: BalanceIn<T>,
		) -> DispatchResult {
			// Origin should be signed.
			let who = ensure_signed(origin.clone())?;

			// member should be present in community members list
			let community_member = CommunityMembers::<T>::get();
			let _is_present = community_member
				.binary_search(&who)
				.map_err(|_| Error::<T>::MemberIsNotPresentInCommunity)?;

			ensure!(!Voting::<T>::contains_key(&proposal_hash), Error::<T>::ProposalAlreadyExist);

			let info = { ProposalInfo { amount, title } };
			// Add Proposal
			<Proposal<T>>::insert(proposal_hash, info);

			// initially the votes will be null for any proposal.
			let votes = { Votes { ayes: vec![], nays: vec![] } };
			<Voting<T>>::insert(proposal_hash, votes);

			let mut user = Vec::new();
			user.push(&who);

			<FundSeekerAccounts<T>>::insert(proposal_hash, user.clone());
			Self::deposit_event(Event::ProposalAdded);
			Ok(())
		}

		/// Approve the Proposal propose by any community member
		/// Only committee can propose the proposal
		#[pallet::weight(10_000_000)]
		pub fn approve_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
			approve: Vote,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let committee_members = CommitteeMembers::<T>::get();
			let _is_present = committee_members
				.binary_search(&who)
				.map_err(|_| Error::<T>::MemberIsNotPresentInCommittee)?;

			let total_approvers = Approvers::<T>::get(&proposal_hash);

			let location =
				total_approvers.binary_search(&who).err().ok_or(Error::<T>::AlreadyApproved)?;

			// Check proposal is present or not.
			let mut voting = Self::voting(&proposal_hash).ok_or(Error::<T>::ProposalMissing)?;

			// Cast vote on particular proposal
			match approve {
				// if recent member cast vote as 'aye'
				Vote::Aye => {
					voting.ayes.push(who.clone());
					<Voting<T>>::insert(proposal_hash, voting.clone());
				},
				_ => {
					voting.nays.push(who.clone());
					<Voting<T>>::insert(proposal_hash, voting.clone());
				},
			}
			// fetch all the approvers.
			let mut members = Approvers::<T>::get(proposal_hash);
			members.insert(location, who.clone());
			// add new approver
			Approvers::<T>::insert(proposal_hash, members);

			// Time after which the decision will make whether funds will be transfer or not
			let expire_time = T::TimeDuration::get();
			// Record the current BlockNumber and set the target BlockNumber.
			let transaction_blocknumber =
				frame_system::Pallet::<T>::block_number() + expire_time.into();
			TransferTime::<T>::insert(transaction_blocknumber, &proposal_hash);

			// fetch total no. of ayes and nays
			let ayes_votes = voting.ayes.len() as MemberCount;
			let nays_votes = voting.nays.len() as MemberCount;

			Self::deposit_event(Event::Approved {
				account: who,
				proposal_hash,
				voted: approve,
				ayes: ayes_votes,
				nays: nays_votes,
			});

			Ok(())
		}

		/// Set the Account from where the funds will be transferred.
		/// Only sudo is allowed to make this happen.
		#[pallet::weight(10_000_000)]
		pub fn add_pot_account(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			ensure_root(origin.clone())?;
			let mut accounts = Vec::new();
			accounts.push(who.clone());
			PotAccount::<T>::put(accounts);
			Ok(())
		}

		/// Any Community member can fund to the pot account.
		#[pallet::weight(10_000_000)]
		pub fn fund_pot_account(
			origin: OriginFor<T>,
			who: T::AccountId,
			amount: BalanceIn<T>,
		) -> DispatchResult {
			ensure_signed(origin.clone())?;
			// member should be present in community members list
			let community_member = CommunityMembers::<T>::get();
			let _is_present = community_member
				.binary_search(&who)
				.map_err(|_| Error::<T>::MemberIsNotPresentInCommunity)?;
			let accounts = PotAccount::<T>::get();
			let pot_account = accounts[0].clone();
			T::Currency::transfer(&who, &pot_account, amount, ExistenceRequirement::KeepAlive)?;
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// transfer the funds if the conditions are satisfied.
	/// Funds will be transferred from pot account to the proposer account.
	pub fn transfer_funds(proposal_id: T::Hash) -> DispatchResult {
		// Check proposal is present
		let voting = Self::voting(&proposal_id).ok_or(Error::<T>::ProposalMissing)?;

		// fetch total no of ayes on a particular proposal
		let no_of_ayes = voting.ayes.len() as MemberCount;
		let no_of_committee_members = CommitteeMembers::<T>::get();
		let proposal_info = Proposal::<T>::get(&proposal_id).ok_or(Error::<T>::ProposalMissing)?;
		// Fetch the amount to transfer
		let amount_to_transfer = proposal_info.amount;
		// Fetch Proposer's account
		let destination_accounts_list = FundSeekerAccounts::<T>::get(proposal_id);
		let destination_account = destination_accounts_list[0].clone();
		// Fetch pot accounts(source account)
		let pot_accounts = PotAccount::<T>::get();
		let source = pot_accounts[0].clone();

		// If all the committee members approve the proposal then only funds will be transferred.
		if no_of_ayes == no_of_committee_members.len() as u32 {
			T::Currency::transfer(
				&source,
				&destination_account,
				amount_to_transfer,
				ExistenceRequirement::KeepAlive,
			)?;
			Self::deposit_event(Event::FundTransfer);
		} else {
			// if condition is not satisfied.
			Self::deposit_event(Event::FundTransferDeclined);
		}

		Ok(())
	}
}
