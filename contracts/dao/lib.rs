#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
pub mod dao {
    use ink::storage::Mapping;
    use ink::env::call;
    use openbrush::contracts::traits::psp22::*;
    use scale::{
        Decode,
        Encode,
    };

    // type to track proposals
    pub type ProposalId = u64;

    #[derive(Encode, Decode)]
    #[cfg_attr(feature = "std", derive(Debug, PartialEq, Eq, scale_info::TypeInfo))]
    pub enum VoteType {
        For,
        Against
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum GovernorError {
        AmountShouldNotBeZero,
        DurationError,
        ProposalNotFound,
        ProposalAlreadyExecuted,
        VotePeriodEnded,
        AlreadyVoted,
        QuorumNotReached,
        ProposalNotAccepted,
        TransferFailed,
        AmountExceedContractBalance,
        CallToTokenFailed,
    }

    #[derive(Encode, Decode)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink::storage::traits::StorageLayout
        )
    )]
    pub struct Proposal {
        to: AccountId,
        vote_start: u64,
        vote_end: u64,
        executed: bool,
        amount: Balance,
    }

    #[derive(Encode, Decode, Default)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink::storage::traits::StorageLayout
        )
    )]
    pub struct ProposalVote {
        quorum: u8,
    }

    #[ink(storage)]
    pub struct Governor {
        proposals: Mapping<ProposalId, Proposal>,
        proposal_vote_for: Mapping<ProposalId, u128>,
        proposal_vote_against: Mapping<ProposalId, u128>,
        votes: Mapping<(ProposalId, AccountId), Option<()>>,
        next_proposal_id: ProposalId,
        quorum: u8,
        governance_token: AccountId,
    }

    impl Governor {
        #[ink(constructor)]
        pub fn new(governance_token: AccountId, quorum: u8) -> Self {
            Self {
                proposals: Default::default(),
                proposal_vote_for: Default::default(),
                proposal_vote_against: Default::default(),
                votes: Default::default(),
                next_proposal_id: 0,
                quorum,
                governance_token
            }
        }

        #[ink(message)]
        pub fn propose(
            &mut self,
            to: AccountId,
            amount: Balance,
            duration: u64,
        ) -> Result<(), GovernorError> {
            if amount == 0 {
                return Err(GovernorError::AmountShouldNotBeZero)
            }
            if duration == 0 {
                return Err(GovernorError::DurationError)
            }

            let timestamp = self.env().block_timestamp();
            let proposal = Proposal {
                to,
                vote_start: timestamp,
                vote_end: timestamp + (duration * 60),
                executed: false,
                amount
            };
            let proposal_id = self.next_proposal_id;
            self.proposals.insert(proposal_id, &proposal);
            self.next_proposal_id = proposal_id + 1;

            Ok(())
        }

        #[ink(message)]
        pub fn vote(
            &mut self,
            proposal_id: ProposalId,
            vote: VoteType,
        ) -> Result<(), GovernorError> {
            if proposal_id >= self.next_proposal_id {
                return Err(GovernorError::ProposalNotFound)
            }
            let caller = self.env().caller();
            let timestamp = self.env().block_timestamp();
            let proposal = self.get_proposal(proposal_id);

            let proposal = match proposal {
                Some(p) => p,
                None => return Err(GovernorError::ProposalNotFound)
            };
            if proposal.executed {
                return Err(GovernorError::ProposalAlreadyExecuted)
            }
            if timestamp > proposal.vote_end {
                return Err(GovernorError::VotePeriodEnded)
            }

            let proposal_vote = self.votes.get((proposal_id, &caller));
            if let Some(_) = proposal_vote {
                return Err(GovernorError::AlreadyVoted)
            }
            self.votes.insert((proposal_id, &caller), &Some(()));

            let caller_balance = call::build_call::<ink::env::DefaultEnvironment>()
                .call(self.governance_token)
                .gas_limit(0)
                .transferred_value(10)
                .exec_input(
                    call::ExecutionInput::new(call::Selector::new(ink::selector_bytes!("balanceOf")))
                        // .push_arg(42u8)
                        // .push_arg(true)
                        // .push_arg(&[0x10u8; 32])
                )
                .returns::<Balance>()
                .invoke();

            let total_token_supply = call::build_call::<ink::env::DefaultEnvironment>()
            .call(self.governance_token)
            .gas_limit(0)
            .transferred_value(10)
            .exec_input(
                call::ExecutionInput::new(call::Selector::new(ink::selector_bytes!("totalSupply")))
                    // .push_arg(42u8)
                    // .push_arg(true)
                    // .push_arg(&[0x10u8; 32])
            )
            .returns::<Balance>()
            .invoke();

            // scale up
            let factor = 100_000;
            let weight = caller_balance * factor * 100 / total_token_supply;

            // increment vote for or against
            match vote {
                VoteType::For => {
                    let vote_for_count = self.proposal_vote_for.get(proposal_id).unwrap_or_default();
                    self.proposal_vote_for.insert(proposal_id, &(vote_for_count + weight));
                },
                VoteType::Against => {
                    let vote_against_count = self.proposal_vote_against.get(proposal_id).unwrap_or_default();
                    self.proposal_vote_against.insert(proposal_id, &(vote_against_count + weight));
                }
            };
            Ok(())
        }

        #[ink(message)]
        pub fn execute(&mut self, proposal_id: ProposalId) -> Result<(), GovernorError> {
            let next_proposal_id = self.next_proposal_id;
            if proposal_id >= next_proposal_id {
                return Err(GovernorError::ProposalNotFound)
            }
            let proposal = match self.get_proposal(proposal_id) {
                Some(p) => p,
                None => return Err(GovernorError::ProposalNotFound)
            };
            if proposal.executed {
                return Err(GovernorError::ProposalAlreadyExecuted)
            }
            let quorum = self.quorum;
            let proposal_vote_for = self.proposal_vote_for.get(proposal_id).unwrap_or_default();
            let proposal_vote_against = self.proposal_vote_against.get(proposal_id).unwrap_or_default();
            let vote_sum = proposal_vote_for + proposal_vote_against;

            // scale down
            let factor = 100_000;
            if u128::from(quorum) > (vote_sum/factor) {
                return Err(GovernorError::QuorumNotReached)
            }
            if proposal_vote_against > proposal_vote_for {
                return Err(GovernorError::ProposalNotAccepted)
            }
            let proposal_amount = proposal.amount;
            let contract_balance = self.env().balance();
            if proposal_amount > contract_balance {
                return Err(GovernorError::AmountExceedContractBalance)
            }
            // send to proposal recipient
            let recipient = proposal.to;

            if let Err(_) = self.env().transfer(recipient, proposal_amount) {
                return Err(GovernorError::TransferFailed);
            }
            // set executed to true
            let proposal = Proposal {
                executed: true,
                ..proposal
            };
            self.proposals.insert(proposal_id, &proposal);
            Ok(())
        }

        // used for test
        #[ink(message)]
        pub fn now(&self) -> u64 {
            self.env().block_timestamp()
        }

        // get `proposal` from `id`
        #[inline]
        pub fn get_proposal(&self, id: ProposalId) -> Option<Proposal> {
            if id >= self.next_proposal_id {
                return None
            }

            let proposal = self.proposals.get(id);
            return proposal
        }

        // get `id` of the next proposal
        #[inline]
        pub fn next_proposal_id(&self) -> ProposalId {
            self.next_proposal_id
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        const ONE_MINUTE: u64 = 60;

        fn create_contract(initial_balance: Balance) -> Governor {
            let accounts = default_accounts();
            set_sender(accounts.alice);
            set_balance(contract_id(), initial_balance);
            Governor::new(AccountId::from([0x01; 32]), 50)
        }

        fn contract_id() -> AccountId {
            ink::env::test::callee::<ink::env::DefaultEnvironment>()
        }

        fn default_accounts(
        ) -> ink::env::test::DefaultAccounts<ink::env::DefaultEnvironment> {
            ink::env::test::default_accounts::<ink::env::DefaultEnvironment>()
        }

        fn set_sender(sender: AccountId) {
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(sender);
        }

        fn set_balance(account_id: AccountId, balance: Balance) {
            ink::env::test::set_account_balance::<ink::env::DefaultEnvironment>(
                account_id, balance,
            )
        }

        #[ink::test]
        fn propose_works() {
            let accounts = default_accounts();
            let mut governor = create_contract(1000);
            assert_eq!(
                governor.propose(accounts.django, 0, 1),
                Err(GovernorError::AmountShouldNotBeZero)
            );
            assert_eq!(
                governor.propose(accounts.django, 100, 0),
                Err(GovernorError::DurationError)
            );
            let result = governor.propose(accounts.django, 100, 1);
            assert_eq!(result, Ok(()));
            let proposal = governor.get_proposal(0).unwrap();
            let now = governor.now();
            assert_eq!(
                proposal,
                Proposal {
                    to: accounts.django,
                    amount: 100,
                    vote_start: 0,
                    vote_end: now + 1 * ONE_MINUTE,
                    executed: false,
                }
            );
            assert_eq!(governor.next_proposal_id(), 1);
        }

        #[ink::test]
        fn quorum_not_reached() {
            let mut governor = create_contract(1000);
            let result = governor.propose(AccountId::from([0x02; 32]), 100, 1);
            assert_eq!(result, Ok(()));
            let execute = governor.execute(0);
            assert_eq!(execute, Err(GovernorError::QuorumNotReached));
        }
    }
}
