function listResource( resource_type ) {
    let url = "/aws/list?resource=" + resource_type;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = "&nbsp;";
        document.getElementById("main_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function terminateInstance( instance_id ) {
    let url = "/aws/terminate?instance=" + instance_id;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('instances');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function createImage( inst_id, name ) {
    let url = "/aws/create_image?inst_id=" + inst_id + "&name=" + name;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('ami');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteImage( ami ) {
    let url = "/aws/delete_image?ami=" + ami;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('ami');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteVolume( volid ) {
    let url = "/aws/delete_volume?volid=" + volid;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('volume');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function createSnapshot( volid, name ) {
    let url = "/aws/create_snapshot?volid=" + volid;
    if (name) {
        url = url + "&name=" + name;
    }
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('volume');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function modifyVolume( volid ) {
    let key = volid + '_vol_size';
    let vol_size = document.getElementById(key).value;
    let url = "/aws/modify_volume?volid=" + volid + "&size=" + vol_size;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('volume');
    }
    xmlhttp.open("PATCH", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteSnapshot( snapid ) {
    let url = "/aws/delete_snapshot?snapid=" + snapid;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('snapshot');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteEcrImage( repo, id ) {
    let url = "/aws/delete_image?reponame=" + repo + "&imageid=" + id;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('ecr');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function cleanupEcrImages() {
    let url = "/aws/cleanup_ecr_images";
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('ecr');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function editScript( filename ) {
    let url = "/aws/edit_script?filename=" + filename;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
}
function submitFormData( filename ) {
    let url = '/aws/replace_script';
    let text = document.getElementById( 'script_editor_form' ).value;
    let data = JSON.stringify({'filename': filename, 'text': text});
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.onload = function see_result() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('script');
    }
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function createScript() {
    let filename = document.getElementById( 'script_filename' ).value;
    editScript( filename );
}
function deleteScript( filename ) {
    let url = "/aws/delete_script?filename=" + filename;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('script');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function buildSpotRequest( ami, inst, script ) {
    let url = "/aws/build_spot_request";
    if (ami) {
        url = url + "?ami=" + ami;
    } else if (inst) {
        url = url + "?inst=" + inst;
    } else if (script) {
        url = url + "?script=" + script;
    }
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("sub_article").innerHTML = "&nbsp;";
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function updateScriptAndBuildSpotRequest(script) {
    let url = '/aws/replace_script';
    let text = document.getElementById( 'script_editor_form' ).value;
    let data = JSON.stringify({'filename': script, 'text': text});
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.onload = function see_result() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        buildSpotRequest(null, null, script);
    }
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function requestSpotInstance() {
    let url = "/aws/request_spot";

    let ami = document.getElementById('ami').value;
    let instance_type = document.getElementById('instance_type').value;
    let security_group = document.getElementById('security_group').value;
    let script = document.getElementById('script').value;
    let key = document.getElementById('key').value;
    let price = document.getElementById('price').value;
    let name = document.getElementById('name').value;

    let data = JSON.stringify({
        'ami': ami,
        'instance_type': instance_type,
        'security_group': security_group,
        'script': script,
        'key_name': key,
        'price': price,
        'name': name,
    });

    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.onload = function see_result() {
        listResource('instances');
    }
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function cancelSpotRequest(spot_id) {
    let url = "/aws/cancel_spot?spot_id=" + spot_id;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('spot');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function listAllPrices() {
    let url = "/aws/prices";
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = "&nbsp;";
        document.getElementById("main_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listPrices();
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function listPrices() {
    let url = "/aws/prices";
    let search = document.getElementById('inst_fam').value;
    if (search) {
        url = url + "?search=" + search;
    }
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function updateMetadata() {
    let url = "/aws/update";
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = "&nbsp;";
        document.getElementById("main_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function getStatus( instance ) {
    let url = "/aws/instance_status?instance=" + instance;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function runCommand( instance ) {
    let url = "/aws/command";
    let command = document.getElementById( 'command_text' ).value;
    let data = JSON.stringify({
        'instance': instance,
        'command': command,
    });
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function instanceOptions() {
    let inst = document.getElementById("inst_fam").value;
    let url = "/aws/instances?inst=" + inst;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("instance_type").innerHTML = xmlhttp.responseText;
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
}
function noVncTab(url, method) {
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = "&nbsp;";
        document.getElementById("main_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open(method, url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function tagVolume(id) {
    let key = id + '_tag_volume';
    let tag = document.getElementById(key).value;
    let url = "/aws/tag_item?id=" + id + "&tag=" + tag;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('volume');
    }
    xmlhttp.open("PATCH", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function tagSnapshot(id) {
    let key = id + '_tag_snapshot';
    let tag = document.getElementById(key).value;
    let url = "/aws/tag_item?id=" + id + "&tag=" + tag;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('snapshot');
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function createUser( user_name ) {
    let url = "/aws/create_user?user_name=" + user_name;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('user');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteUser( user_name ) {
    let url = "/aws/delete_user?user_name=" + user_name;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('user');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function addUserToGroup( group_name ) {
    let key = group_name + '_user_opt';
    let user_name = document.getElementById( key ).value;
    let url = "/aws/add_user_to_group?user_name=" + user_name + "&group_name=" + group_name;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('group');
    }
    xmlhttp.open("PATCH", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function removeUserFromGroup( user_name ) {
    let key = user_name + '_group_opt';
    let group = document.getElementById( key ).value;
    let url = "/aws/remove_user_from_group?user_name=" + user_name + "&group_name=" + group;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('user');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function createAccessKey( user_name ) {
    let url = "/aws/create_access_key?user_name=" + user_name;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function deleteAccessKey( user_name, access_key_id ) {
    let url = "/aws/delete_access_key?user_name=" + user_name + "&access_key_id=" + access_key_id;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('access-key');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function updateDnsName(zone, dns_name, old_ip, new_ip) {
    let url = "/aws/update_dns_name?zone=" + zone + "&dns_name=" + dns_name + "&old_ip=" + old_ip + "&new_ip=" + new_ip;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('route53');
    }
    xmlhttp.open("PATCH", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function systemdAction(action, service) {
    let url = "/aws/systemd_action?action=" + action + "&service=" + service;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('systemd');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function systemdLogs(service) {
    let url = "/aws/systemd_logs/" + service;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function systemdRestartAll() {
    let url = "/aws/systemd_restart_all";
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('systemd');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function crontabLogs(crontab_type) {
    let url = "/aws/crontab_logs/" + crontab_type;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("garminconnectoutput").innerHTML = "done";
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
    document.getElementById("garminconnectoutput").innerHTML = "running";
}
function emailDetail( id ) {
    let url = `/aws/inbound-email/${id}`;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
}
function deleteEmail( id ) {
    let url = `/aws/inbound-email/${id}`;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('inbound-email');
    }
    xmlhttp.open("DELETE", url, true);
    xmlhttp.send(null);
}
function syncEmail() {
    let url = "/aws/inbound-email/sync";
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("sub_article").innerHTML = xmlhttp.responseText;
        document.getElementById("garminconnectoutput").innerHTML = "done";
        listResource('inbound-email');
    }
    xmlhttp.open("POST", url, true);
    xmlhttp.send(null);
}