# PSScriptAnalyzer settings for the Windows installer scripts.
# Rules excluded here are intentional choices, not oversights:
#   - Write-Host is used deliberately for interactive console output during installation.
#   - $Password must remain [string] because the NSIS installer expects plain-text input.
@{
    ExcludeRules = @(
        'PSAvoidUsingWriteHost',
        'PSAvoidUsingPlainTextForPassword'
    )
}

