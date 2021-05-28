#!/usr/bin/env bash

function error {
    RED='\033[0;31m'
    NO_COLOR='\033[0m'
    echo -e ${RED}ERROR: $*${NO_COLOR} >&2
}

function warning {
    YELLOW='\033[0;33m'
    NO_COLOR='\033[0m'
    echo -e ${YELLOW}WARNING: $*${NO_COLOR} >&2
}

function exit_fail {
    error $*
    exit 1
}

[[ $BASH_VERSION =~ ^5 ]] || exit_fail "This script needs to run with Bash version 5"

tty -s &>/dev/null || exit_fail "This script needs to run interactively. Use 'docker run -ti <image>'"

function read_password {
    local -n opts=$1
    local valid_password
    while true; do
        valid_password=true
        if ${opts[print]}; then
            IFS= read -r -p "Enter password or press ENTER to generate one: " PASSWORD
        else
            IFS= read -r -s -p "Enter password or press ENTER to generate one: " PASSWORD
            echo
        fi
        if [[ ${#PASSWORD} -eq 0 ]]; then
            local output_length=16
            if [[ ${opts[length]} > 16 ]]; then
                output_length=${opts[length]}
            fi
            PASSWORD=$(pwgen -c -n -y -s -B -v -1 $output_length)
            if [[ $? -ne 0 ]]; then
                exit 5
            fi
            echo Generated password: $PASSWORD
            break
        fi
        if [[ ${opts[length]} -eq 0 ]]; then
            : # No complexity requirements (-d or -l 0 argument was used)
        else
            if [[ ${#PASSWORD} -lt ${opts[length]} ]]; then
                valid_password=false
                warning "Password must be at least ${opts[length]} characters long."
            fi
            if [[ ${opts[case]} == "yes" ]] && ! ( [[ "$PASSWORD" =~ [[:upper:]] ]] && [[ "$PASSWORD" =~ [[:lower:]] ]] ); then
                valid_password=false
                warning "Password complexity rules require a mixed case password. So make sure to include both lower and uppercase characters in your password."
            fi
            if [[ ${opts[symbols]} == "yes" && ! ( "$PASSWORD" =~ [[:punct:]] || "$PASSWORD" =~ " " ) ]]; then
                valid_password=false
                warning "Password complexity rules require a symbolic character in the password."
            fi
            if [[ ${opts[digits]} == "yes" && ! "$PASSWORD" =~ [[:digit:]] ]]; then
                valid_password=false
                warning "Password complexity rules require a digit character in the password."
            fi
        fi
        if $valid_password && ${options[print]}; then
            return
        fi
        while true; do
            if ! $valid_password; then
                echo
                warning "Password does not meet the above mentioned password complexity rules!\n To ignore this: Repeat the weak password at the next prompt.\n To be safe: press ENTER to try again."
            fi
            IFS= read -r -s -p "Repeat password (leave blank to re-enter initial password): " _PASSWORD
            echo
            if [[ -z "$_PASSWORD" ]]; then
                break
            elif [[ "$_PASSWORD" = "$PASSWORD" ]]; then
                if ! $valid_password; then
                    warning "Accepted a possibly insecure password."
                fi
                return
            else
                error "Repeated password does not match"
                error "Try again."
            fi
        done
    done
}

function generate_pbkdf2 {
    local -n opts=$1
    local username=$2
    local salt=$(dd if=/dev/urandom bs=1 count=8 2>/dev/null | hexdump -v -e '"\\" "x" 1/1 "%02x"')
    local b64_salt=$(echo -ne $salt | openssl base64 -A)
    local pbkdf2=$(echo -n "$PASSWORD" | nettle-pbkdf2 -i 500000 -l 32 --hex-salt $(echo -ne $salt | xxd -p -c 80) --raw |openssl base64 -A)

    if [[ -n $username ]]; then
        ENTRY="\"username\": \"$username\", \"pbkdf2_salt\": \"$b64_salt\", \"pbkdf2_key\": \"${pbkdf2}\", \"pbkdf2_iter\": ${options[iter]}"
    else
        printf "pbkdf2_salt: %s\npbkdf2_key: %s\n" $b64_salt $pbkdf2
    fi
}

function validate_yes_no {
    if [[ $1 =~ ^(yes|no)$ ]]; then
        return 0
    else
        return 1
    fi
}

function usage {
    cat <<USAGE
Usage: $(basename $0) [-l length] [-m length] [-s yes|no] [-c yes|no] [-d yes|no] [-i iter] [-n] [-p] [-u] [-h]

Flags
    -h            Show this summary
    -n            Disable password complexity check (complexity options will be ignored)
    -u            Generate copy-pastable JSON credentials output for one or more users directly usable in unFTP
    -p            Don't hide password input

Options
    -l length     Minimum length requirement (default: 12)
    -m length     Maximum length requirement (default: none)
    -s yes|no     Require at least one symbol (default: yes)
    -d yes|no     Require at least one digit (default: yes)
    -c yes|no     Require mixed case (default: yes)
    -i iterations The number of iterations for PBKDF2 (default: 500000)
USAGE
}

declare -A options
options[length]=12
options[symbols]=yes
options[digits]=yes
options[case]=yes
options[print]=false
options[iter]=500000
while getopts ":l:m:s:c:d:i:nuph" arg; do
    case $arg in
        h)
            usage
            exit 0
            ;;
        l)
            options[length]=$OPTARG
            ;;
        m)
            options[maxlength]=$OPTARG
            ;;
        s)
            validate_yes_no $OPTARG || exit_fail "Invalid param for -${arg}: valid values are 'yes' or 'no'"
            options[symbols]=$OPTARG
            ;;
        c)
            validate_yes_no $OPTARG || exit_fail "Invalid param for -${arg}: valid values are 'yes' or 'no'"
            options[case]=$OPTARG
            ;;
        d)
            validate_yes_no $OPTARG || exit_fail "Invalid param for -${arg}: valid values are 'yes' or 'no'"
            options[digits]=$OPTARG
            ;;
        i)
            options[iter]=$OPTARG
            ;;
        n)
            options[length]=0
            ;;
        u)
            GENERATE_JSON=true
            ;;
        p)
            options[print]=true
            ;;
        :)
            error "$0: Must supply an argument to -$OPTARG"
            usage
            exit 2
            ;;
        ?)
            error "Invalid option: -${OPTARG}"
            usage
            exit 2
            ;;
    esac
done

if [[ -z $GENERATE_JSON ]]; then
    read_password options
    generate_pbkdf2 options
    exit 0
else
    read -r -p "Enter username or press ENTER to finish: " USERNAME
    if [[ -z $USERNAME ]]; then
        exit 0
    fi
    read_password options
    generate_pbkdf2 options $USERNAME
    json="[ { $ENTRY }"
    while [[ -n $USERNAME ]]; do
        jq <<<"$json ]"
        read -r -p "Enter username or press ENTER to finish: " USERNAME
        if [[ -z $USERNAME ]]; then
            break
        fi
        read_password options
        generate_pbkdf2 options $USERNAME
        json+=",{ $ENTRY }"
    done
    jq <<<"${json} ]"
fi

