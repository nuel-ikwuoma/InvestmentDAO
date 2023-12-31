// Implement PSP2 + PSP22Metadata
#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub use self::token::GovernanceTokenRef;

#[openbrush::implementation(PSP22, PSP22Metadata)]
#[openbrush::contract]
pub mod token {
    use openbrush::traits::Storage;

    #[ink(storage)]
    #[derive(Default, Storage)]
    pub struct GovernanceToken {
    	#[storage_field]
		psp22: psp22::Data,
		#[storage_field]
		metadata: metadata::Data,
    }


    
    impl GovernanceToken {
        #[ink(constructor)]
        pub fn new(initial_supply: Balance, name: Option<String>, symbol: Option<String>, decimal: u8) -> Self {
            let mut _instance = Self::default();
			psp22::Internal::_mint_to(&mut _instance, Self::env().caller(), initial_supply).expect("Should mint"); 
			_instance.metadata.name.set(&name);
			_instance.metadata.symbol.set(&symbol);
			_instance.metadata.decimals.set(&decimal);
			_instance
        }

        #[ink(message)]
        pub fn balance_of(&self, owner: AccountId) -> Balance {
            psp22::Internal::_balance_of(self, &owner)
        }

        #[ink(message)]
        pub fn total_supply(&self) -> Balance {
            psp22::Internal::_total_supply(self)
            // PSP22::total_supply(self)
        }
    }
}