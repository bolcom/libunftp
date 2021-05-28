#!/usr/bin/env bash

function error {
    RED='\033[0;31m'
    NO_COLOR='\033[0m'
    echo -e ${RED}$*${NO_COLOR} >&2
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
        read -s -p "Enter password or press ENTER to generate one: " PASSWORD
        echo
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
                error "Password must be at least ${opts[length]} characters long."
            fi
            if [[ ${opts[case]} == "yes" ]] && ! ( [[ $PASSWORD =~ [[:upper:]] ]] && [[ $PASSWORD =~ [[:lower:]] ]] ); then
                valid_password=false
                error "Password complexity rules require a mixed case password. So make sure to include both lower and uppercase characters in your password."
            fi
            if [[ ${opts[symbols]} == "yes" && ! $PASSWORD =~ [[:punct:]] ]]; then
                valid_password=false
                error "Password complexity rules require a symbolic character in the password."
            fi
            if [[ ${opts[digits]} == "yes" && ! $PASSWORD =~ [[:digit:]] ]]; then
                valid_password=false
                error "Password complexity rules require a digit character in the password."
            fi
        fi
        if $valid_password; then
            while true; do
                read -s -p "Repeat password (leave blank to re-enter initial password): " _PASSWORD
                echo
                if [[ -z $_PASSWORD ]]; then
                    break
                elif [[ $_PASSWORD = $PASSWORD ]]; then
                    return
                else
                    error "Repeated password does not match"
                    error "Try again."
                fi
            done
        else
            echo
            echo "Try again with above requirements satisfied."
        fi
    done
}

function generate_pbkdf2 {
    local -n opts=$1
    local username=$2
    local salt=$(dd if=/dev/urandom bs=1 count=8 2>/dev/null | hexdump -v -e '"\\" "x" 1/1 "%02x"')
    local b64_salt=$(echo -ne $salt | openssl base64 -A)
    local pbkdf2=$(echo -n $PASSWORD | nettle-pbkdf2 -i 500000 -l 32 --hex-salt $(echo -n $salt | xxd -p -c 80) --raw |openssl base64 -A)

    if [[ -n $username ]]; then
        ENTRY="\"username\": \"$username\", \"pbkdf2_salt\": \"$b64_salt\", \"pbkdf2_key\": \"${pbkdf2}\", \"pbkdf2_iter\": ${options[iter]}"
    else
        printf "pbkdf2_salt: %s\npbkdf2_key: %s\n" $b64_salt $pbkdf2
    fi
}

function usage {
    cat <<USAGE
Usage: $(basename $0) [-l length] [-m length] [-s yes|no] [-c yes|no] [-d yes|no] [-n] [-p]

Flags
    -h            Show this summary
    -n            Disable password complexity check (complexity options will be ignored)
    -u            Generate copy-pastable JSON credentials output for one or more users directly usable in unFTP

Options
    -l length     Minimum length requirement (default: 12)
    -m length     Maximum length requirement (default: none)
    -s yes|no     Require at least one symbol (default: yes)
    -d yes|no     Require at least one digit (default: yes)
    -c yes|no     Require mixed case (default: yes)
USAGE
}

declare -A options
options[length]=12
options[symbols]=yes
options[digits]=yes
options[case]=yes
options[iter]=500000
while getopts ":l:m:s:c:d:i:nuh" arg; do
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
            options[symbols]=$OPTARG
            ;;
        c)
            options[case]=$OPTARG
            ;;
        d)
            options[digits]=$OPTARG
            ;;
        n)
            options[length]=0
            ;;
        u)
            GENERATE_JSON=true
            ;;
        i)
            options[iter]=$OPTARG
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
    read -p "Enter username or press ENTER to finish: " USERNAME
    if [[ -z $USERNAME ]]; then
        exit 0
    fi
    read_password options
    generate_pbkdf2 options $USERNAME
    json="[ { $ENTRY }"
    while [[ -n $USERNAME ]]; do
        jq <<<"$json ]"
        read -p "Enter username or press ENTER to finish: " USERNAME
        if [[ -z $USERNAME ]]; then
            break
        fi
        read_password options
        generate_pbkdf2 options $USERNAME
        json+=",{ $ENTRY }"
    done
    jq <<<"${json} ]"
fi

