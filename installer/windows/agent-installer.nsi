;includes
!include nsDialogs.nsh
!include LogicLib.nsh
!include FileFunc.nsh

!insertmacro GetParameters
!insertmacro GetOptions

!define APPNAME "OBAS Agent"
!define COMPANYNAME "Filigran"
!define DESCRIPTION "Filigran's agent for OpenBAS"
# These will be displayed by the "Click here for support information" link in "Add/Remove Programs"
# It is possible to use "mailto:" links in here to open the email client
!define HELPURL "https://filigran.io/" # "Support Information" link
!define UPDATEURL "https://filigran.io/" # "Product Updates" link
!define ABOUTURL "https://filigran.io/" # "Publisher" link

# Windows Service
!define displayName "${APPNAME} Service"
 
RequestExecutionLevel admin ;Require admin rights on NT6+ (When UAC is turned on)
 
# rtf or txt file - remember if it is txt, it must be in the DOS text format (\r\n)
LicenseData "license.txt"
# This will be in the installer/uninstaller's title bar
Name "${COMPANYNAME} - ${APPNAME}"
Icon "openbas.ico"
outFile "agent-installer.exe"
 
; page definition
page license
page directory
Page custom nsDialogsConfig nsDialogsPageLeave
Page instfiles
 
!macro VerifyUserIsAdmin
UserInfo::GetAccountType
pop $0
${If} $0 != "admin" ;Require admin rights on NT4+
        messageBox mb_iconstop "Administrator rights required!"
        setErrorLevel 740 ;ERROR_ELEVATION_REQUIRED
        quit
${EndIf}
!macroend

Var Dialog
Var LabelURL
Var /GLOBAL ConfigURL
Var LabelToken
Var /GLOBAL ConfigToken
Var LabelUnsecuredCertificate
Var /GLOBAL ConfigUnsecuredCertificate
Var LabelWithProxy
Var /GLOBAL ConfigWithProxy
Var /GLOBAL ConfigInstallDir
Var /GLOBAL ConfigServiceName
var /GLOBAL ServiceName

Function ExtractParameter
    ; Input: Parameter name in $0, Full command line in $1
    ; Output: Parameter value in $0

    Push $1
    Push $2
    Push $3
    Push $4
    Push $5
    Push $6
    Push $7
    Push $8

    StrCpy $2 $0  ; Parameter name to find
    StrCpy $3 $1  ; Command line
    StrCpy $0 ""  ; Clear output

    ; Check if parameter is Windows-quoted: "~PARAM=value"
    Push $3
    Push '"~$2='
    Call StrStr
    Pop $4

    StrCmp $4 "" try_normal 0
    ; It's Windows-quoted, skip the quote
    StrCpy $4 $4 "" 1
    StrLen $5 "~$2="
    StrCpy $4 $4 "" $5

    ; Extract until the closing quote
    StrCpy $0 ""
    StrCpy $6 0
quoted_extract:
    StrCpy $5 $4 1 $6
    StrCmp $5 "" done
    StrCmp $5 '"' done  ; Stop at closing quote
    StrCpy $0 "$0$5"
    IntOp $6 $6 + 1
    Goto quoted_extract

try_normal:
    ; Try normal format: ~PARAM=value
    Push $3
    Push "~$2="
    Call StrStr
    Pop $4

    StrCmp $4 "" done  ; Parameter not found

    ; Skip past parameter name and =
    StrLen $5 "~$2="
    StrCpy $4 $4 "" $5

    ; Extract value - handle quoted and unquoted
    StrCpy $5 $4 1  ; First character
    StrCmp $5 '"' normal_quoted_value
    StrCmp $5 "'" normal_single_quoted_value

    ; Unquoted value - read until space or ~ or end
    StrCpy $0 ""
    StrCpy $6 0
normal_unquoted_loop:
    StrCpy $5 $4 1 $6
    StrCmp $5 "" done
    StrCmp $5 " " done
    StrCmp $5 "~" done
    StrCpy $0 "$0$5"
    IntOp $6 $6 + 1
    Goto normal_unquoted_loop

normal_quoted_value:
    ; Skip opening quote
    StrCpy $4 $4 "" 1
    StrCpy $0 ""
    StrCpy $6 0
normal_quoted_loop:
    StrCpy $5 $4 1 $6
    StrCmp $5 "" done
    StrCmp $5 '"' done
    StrCpy $0 "$0$5"
    IntOp $6 $6 + 1
    Goto normal_quoted_loop

normal_single_quoted_value:
    ; Skip opening quote
    StrCpy $4 $4 "" 1
    StrCpy $0 ""
    StrCpy $6 0
normal_single_quoted_loop:
    StrCpy $5 $4 1 $6
    StrCmp $5 "" done
    StrCmp $5 "'" done
    StrCpy $0 "$0$5"
    IntOp $6 $6 + 1
    Goto normal_single_quoted_loop

done:
    ; Trim trailing spaces
trim_end:
    StrLen $5 $0
    IntCmp $5 0 really_done
    IntOp $5 $5 - 1
    StrCpy $6 $0 1 $5
    StrCmp $6 " " 0 really_done
    StrCpy $0 $0 $5
    Goto trim_end

really_done:
    Pop $8
    Pop $7
    Pop $6
    Pop $5
    Pop $4
    Pop $3
    Pop $2
    Pop $1
FunctionEnd

; StrStr function
Function StrStr
    Exch $R1 ; st=haystack,old$R1, $R1=needle
    Exch     ; st=old$R1,haystack
    Exch $R2 ; st=old$R1,old$R2, $R2=haystack
    Push $R3
    Push $R4
    Push $R5
    StrLen $R3 $R1
    StrCpy $R4 0
    ; $R1=needle
    ; $R2=haystack
    ; $R3=len(needle)
    ; $R4=cnt
    ; $R5=tmp
loop:
    StrCpy $R5 $R2 $R3 $R4
    StrCmp $R5 $R1 done
    StrCmp $R5 "" done
    IntOp $R4 $R4 + 1
    Goto loop
done:
    StrCpy $R1 $R2 "" $R4
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Exch $R1
FunctionEnd

function .onInit
    setShellVarContext all
    !insertmacro VerifyUserIsAdmin

    ${GetParameters} $R0

    ; Extract parameters using custom function
    Push $R0
    Pop $1

    StrCpy $0 "OPENBAS_URL"
    Call ExtractParameter
    StrCpy $ConfigURL $0

    StrCpy $0 "ACCESS_TOKEN"
    Call ExtractParameter
    StrCpy $ConfigToken $0

    StrCpy $0 "UNSECURED_CERTIFICATE"
    Call ExtractParameter
    StrCpy $ConfigUnsecuredCertificate $0

    StrCpy $0 "WITH_PROXY"
    Call ExtractParameter
    StrCpy $ConfigWithProxy $0

    StrCpy $0 "SERVICE_NAME"
    Call ExtractParameter
    StrCpy $ConfigServiceName $0

    StrCpy $0 "INSTALL_DIR"
    Call ExtractParameter
    StrCpy $ConfigInstallDir $0

    ; Set defaults if not provided
    ${If} $ConfigServiceName == ""
        StrCpy $ConfigServiceName "OBASAgentService"
    ${EndIf}
    ${If} $ConfigInstallDir == ""
        StrCpy $ConfigInstallDir "$PROGRAMFILES\${COMPANYNAME}\${APPNAME}"
    ${EndIf}

    StrCpy $ServiceName $ConfigServiceName
    StrCpy $INSTDIR $ConfigInstallDir
functionEnd

Var ConfigURLForm
Var ConfigTokenForm
Var ConfigUnsecuredCertificateForm
Var ConfigWithProxyForm
Function nsDialogsConfig

  ; disable next button
  GetDlgItem $0 $HWNDPARENT 1
  EnableWindow $0 0

	nsDialogs::Create 1018
	Pop $Dialog

	${If} $Dialog == error
		Abort
	${EndIf}

  ${NSD_CreateLabel} 0 0 100% 12u "OpenBAS URL *"
	Pop $LabelURL
	${NSD_CreateText} 0 13u 100% 12u "http://localhost:3001"
	Pop $ConfigURLForm
  ${NSD_CreateLabel} 0 30u 100% 12u "Access token *"
	Pop $LabelToken
	${NSD_CreatePassword} 0 42u 100% 12u ""
	Pop $ConfigTokenForm
  ${NSD_CreateLabel} 0 55u 100% 12u "Unsecured certificate (true or false) *"
    Pop $LabelUnsecuredCertificate
    ${NSD_CreateText} 0 67u 100% 12u "false"
    Pop $ConfigUnsecuredCertificateForm
  ${NSD_CreateLabel} 0 85u 100% 12u "Env with proxy (true or false) *"
    Pop $LabelWithProxy
    ${NSD_CreateText} 0 97u 100% 12u "false"
    Pop $ConfigWithProxyForm

  ${NSD_OnChange} $ConfigURLForm onFieldChange
  ${NSD_OnChange} $ConfigTokenForm onFieldChange
  ${NSD_OnChange} $ConfigUnsecuredCertificateForm onFieldChange
  ${NSD_OnChange} $ConfigWithProxyForm onFieldChange

	nsDialogs::Show
FunctionEnd

Function onFieldChange
  ; save in register the values entered by user
  ${NSD_GetText} $ConfigURLForm $ConfigURL
  ${NSD_GetText} $ConfigTokenForm $ConfigToken
  ${NSD_GetText} $ConfigUnsecuredCertificateForm $ConfigUnsecuredCertificate
  ${NSD_GetText} $ConfigWithProxyForm $ConfigWithProxy

  ; enable next button if both defined 
  ${If} $ConfigURL != "" 
  ${AndIf} $ConfigToken != ""
  ${AndIf} $ConfigUnsecuredCertificate != ""
  ${AndIf} $ConfigWithProxy != ""
    GetDlgItem $0 $HWNDPARENT 1
    EnableWindow $0 1
  ${Else}
    GetDlgItem $0 $HWNDPARENT 1
    EnableWindow $0 0
  ${EndIf}

FunctionEnd

Function nsDialogsPageLeave
  ; check values are defined
  ${If} $ConfigURL == ""
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing URL"
	  Abort
  ${EndIf}

  ${If} $ConfigToken == ""
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing Token"
	  Abort
  ${EndIf}

  ${If} $ConfigUnsecuredCertificate != "false"
  ${AndIf} $ConfigUnsecuredCertificate != "true"
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing false or true value for unsecured certificate"
  	  Abort
  ${EndIf}

  ${If} $ConfigWithProxy != "false"
  ${AndIf} $ConfigWithProxy != "true"
    MessageBox MB_OK|MB_ICONEXCLAMATION "Missing false or true value for env with proxy"
      Abort
  ${EndIf}
FunctionEnd

section "install"
  # Files for the install directory - to build the installer, these should be in the same directory as the install script (this file)
  setOutPath $INSTDIR

  ;stopping existing service
  ExecWait 'sc stop $ServiceName' $0

  ;deleting existing service
  ExecWait 'sc delete $ServiceName' $0

  # Files added here should be removed by the uninstaller (see section "uninstall")
  file "..\..\target\release\openbas-agent.exe"
  file "openbas.ico"
  
  ; write agent config file
  FileOpen $4 "$INSTDIR\openbas-agent-config.toml" w
    FileWrite $4 "debug=false$\r$\n"
    FileWrite $4 "[openbas]$\r$\n"
    FileWrite $4 "url = $\"$ConfigURL$\"$\r$\n"
    FileWrite $4 "token = $\"$ConfigToken$\"$\r$\n"
    FileWrite $4 "unsecured_certificate = $ConfigUnsecuredCertificate$\r$\n"
    FileWrite $4 "with_proxy = $ConfigWithProxy$\r$\n"
    FileWrite $4 "installation_mode = $\"service$\"$\r$\n"
    FileWrite $4 "service_name = $\"$ConfigServiceName$\"$\r$\n"
    FileWrite $4 "service_full_name = $\"$ServiceName$\"$\r$\n"
    FileWrite $4 "$\r$\n" ; newline
  FileClose $4

  ; register windows service
  ExecWait 'sc create $ServiceName error="severe" displayname="${displayName}" type="own" start="auto" binpath="$INSTDIR\openbas-agent.exe"'

  ; configure restart in case of failure
  ExecWait 'sc failure $ServiceName reset= 0 actions= restart/60000/restart/60000/restart/60000'

  ; start the service
  ExecWait 'sc start $ServiceName'

  # Uninstaller - See function un.onInit and section "uninstall" for configuration
  writeUninstaller "$INSTDIR\uninstall.exe"

sectionEnd
 
# Uninstaller

Function un.ReadServiceNameFromToml
    StrCpy $ServiceName "" ; Reset
    FileOpen $0 "$INSTDIR\openbas-agent-config.toml" r

read_loop:
    FileRead $0 $1
    StrCmp $1 "" close_file ; End of file

    ; Remove trailing newline/carriage return
    Push $1
    Call un.StripNewline
    Pop $1

    ; Skip empty lines
    StrCmp $1 "" read_loop

    ; Debug: Show every cleaned line being read (optional - can be removed)
    ; MessageBox MB_OK "Line: '$1'"

    ; Trim leading spaces
trim_spaces:
    StrCpy $2 $1 1
    StrCmp $2 " " 0 after_trim
    StrCpy $1 $1 "" 1
    Goto trim_spaces

after_trim:
    ; See if this is the service_name line
    StrCpy $2 $1 12
    StrCmp $2 "service_name" match
    Goto read_loop

match:
    ; Now, extract value after '='
    StrCpy $3 $1
    StrCpy $4 0

find_eq:
    StrCpy $5 $3 1 $4
    StrCmp $5 "" read_loop  ; If no '=' found, continue to next line
    StrCmp $5 "=" found_eq
    IntOp $4 $4 + 1
    Goto find_eq

found_eq:
    IntOp $4 $4 + 1
    StrCpy $6 $3 "" $4 ; everything after '='

    ; Remove leading space(s)
trim_val:
    StrCpy $7 $6 1
    StrCmp $7 " " 0 trim_val_done
    StrCpy $6 $6 "" 1
    Goto trim_val

trim_val_done:
    ; Remove quotes if present
    StrCpy $7 $6 1
    StrCmp $7 '"' 0 check_end_quote
    StrCpy $6 $6 "" 1

check_end_quote:
    StrLen $8 $6
    IntCmp $8 0 skip_quote skip_quote
    IntOp $8 $8 - 1
    StrCpy $7 $6 1 $8
    StrCmp $7 '"' 0 skip_quote
    StrCpy $6 $6 $8

skip_quote:
    StrCpy $ServiceName $6
    ; MessageBox MB_OK "Extracted ServiceName: '$ServiceName'"
    Goto close_file

close_file:
    FileClose $0
FunctionEnd

Function un.StripNewline
    Exch $0
    Push $1
    Push $2

again:
    StrLen $2 $0
    IntCmp $2 0 done done
    IntOp $2 $2 - 1
    StrCpy $1 $0 1 $2
    StrCmp $1 "$\r" strip
    StrCmp $1 "$\n" strip
    Goto done

strip:
    StrCpy $0 $0 $2
    Goto again

done:
    Pop $2
    Pop $1
    Exch $0
FunctionEnd

function un.onInit
	SetShellVarContext all

	# Get ServiceName
	Call un.ReadServiceNameFromToml
 
	#Verify the uninstaller - last chance to back out
	MessageBox MB_OKCANCEL "Permanently remove ${APPNAME}?" IDOK next
		Abort
	next:
	!insertmacro VerifyUserIsAdmin
functionEnd

section "uninstall"
  ;stopping existing service
  ExecWait 'sc stop $ServiceName' $0

  ; unregister service
  ExecWait 'sc delete $ServiceName'

  ; delete everything
	RMDir /r $INSTDIR
sectionEnd