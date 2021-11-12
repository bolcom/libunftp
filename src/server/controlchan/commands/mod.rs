//! This module contains the implementations for the FTP commands defined in
//!
//! - [RFC 959 - FTP](https://tools.ietf.org/html/rfc959)
//! - [RFC 3659 - Extensions to FTP](https://tools.ietf.org/html/rfc3659)
//! - [RFC 2228 - FTP Security Extensions](https://tools.ietf.org/html/rfc2228)

mod abor;
mod acct;
mod allo;
mod auth;
mod ccc;
mod cdup;
mod cwd;
mod dele;
mod feat;
mod help;
mod list;
mod md5;
mod mdtm;
mod mkd;
mod mode;
mod nlst;
mod noop;
mod opts;
mod pass;
mod pasv;
mod pbsz;
mod port;
mod prot;
mod pwd;
mod quit;
mod rest;
mod retr;
mod rmd;
mod rnfr;
mod rnto;
mod size;
mod stat;
mod stor;
mod stou;
mod stru;
mod syst;
mod type_;
mod user;

pub use self::md5::Md5;
pub use abor::Abor;
pub use acct::Acct;
pub use allo::Allo;
pub use auth::{Auth, AuthParam};
pub use ccc::Ccc;
pub use cdup::Cdup;
pub use cwd::Cwd;
pub use dele::Dele;
pub use feat::Feat;
pub use help::Help;
pub use list::List;
pub use mdtm::Mdtm;
pub use mkd::Mkd;
pub use mode::{Mode, ModeParam};
pub use nlst::Nlst;
pub use noop::Noop;
pub use opts::{Opt, Opts};
pub use pass::Pass;
pub use pasv::{make_pasv_reply, Pasv};
pub use pbsz::Pbsz;
pub use port::Port;
pub use prot::{Prot, ProtParam};
pub use pwd::Pwd;
pub use quit::Quit;
pub use rest::Rest;
pub use retr::Retr;
pub use rmd::Rmd;
pub use rnfr::Rnfr;
pub use rnto::Rnto;
pub use size::Size;
pub use stat::Stat;
pub use stor::Stor;
pub use stou::Stou;
pub use stru::{Stru, StruParam};
pub use syst::Syst;
pub use type_::Type;
pub use user::User;

pub use self::md5::Md5Handler;
pub use abor::AborHandler;
pub use acct::AcctHandler;
pub use allo::AlloHandler;
pub use auth::AuthHandler;
pub use ccc::CccHandler;
pub use cdup::CdupHandler;
pub use cwd::CwdHandler;
pub use dele::DeleHandler;
pub use feat::FeatHandler;
pub use help::HelpHandler;
pub use list::ListHandler;
pub use mdtm::MdtmHandler;
pub use mkd::MkdHandler;
pub use mode::ModeHandler;
pub use nlst::NlstHandler;
pub use noop::NoopHandler;
pub use opts::OptsHandler;
pub use pass::PassHandler;
pub use pasv::PasvHandler;
pub use pbsz::PbszHandler;
pub use port::PortHandler;
pub use prot::ProtHandler;
pub use pwd::PwdHandler;
pub use quit::QuitHandler;
pub use rest::RestHandler;
pub use retr::RetrHandler;
pub use rmd::RmdHandler;
pub use rnfr::RnfrHandler;
pub use rnto::RntoHandler;
pub use size::SizeHandler;
pub use stat::StatHandler;
pub use stor::StorHandler;
pub use stou::StouHandler;
pub use stru::StruHandler;
pub use syst::SystHandler;
pub use type_::TypeHandler;
pub use user::UserHandler;

use downcast_rs::{impl_downcast, DowncastSync};
use std::fmt::Debug;

pub trait Command: Debug + DowncastSync {
    /// The name of the command
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}
impl_downcast!(sync Command);
