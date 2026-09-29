# Shared by the hooks in this directory. Identities that must never appear
# in this repository, as an extended regex matched case-insensitively
# against an email address.
banned_email='@([a-z0-9-]+\.)*qogni\.com'

is_banned() {
	[ -n "$1" ] && printf '%s\n' "$1" | grep -qiE "$banned_email"
}

refuse() {
	echo "$1" >&2
	echo "Set the right identity: git config user.email vjroger@gmail.com" >&2
	echo "Do not bypass this with --no-verify." >&2
	exit 1
}
