# \UsersApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**check_current_user_follows**](UsersApi.md#check_current_user_follows) | **GET** /me/following/contains | Check If User Follows Artists or Users 
[**check_if_user_follows_playlist**](UsersApi.md#check_if_user_follows_playlist) | **GET** /playlists/{playlist_id}/followers/contains | Check if Current User Follows Playlist 
[**follow_artists_users**](UsersApi.md#follow_artists_users) | **PUT** /me/following | Follow Artists or Users 
[**follow_playlist**](UsersApi.md#follow_playlist) | **PUT** /playlists/{playlist_id}/followers | Follow Playlist 
[**get_current_users_profile**](UsersApi.md#get_current_users_profile) | **GET** /me | Get Current User's Profile 
[**get_followed**](UsersApi.md#get_followed) | **GET** /me/following | Get Followed Artists 
[**get_list_users_playlists**](UsersApi.md#get_list_users_playlists) | **GET** /users/{user_id}/playlists | Get User's Playlists 
[**get_users_profile**](UsersApi.md#get_users_profile) | **GET** /users/{user_id} | Get User's Profile 
[**get_users_top_artists_and_tracks**](UsersApi.md#get_users_top_artists_and_tracks) | **GET** /me/top/{type} | Get User's Top Items 
[**unfollow_artists_users**](UsersApi.md#unfollow_artists_users) | **DELETE** /me/following | Unfollow Artists or Users 
[**unfollow_playlist**](UsersApi.md#unfollow_playlist) | **DELETE** /playlists/{playlist_id}/followers | Unfollow Playlist 



## check_current_user_follows

> Vec<bool> check_current_user_follows(r#type, ids)
Check If User Follows Artists or Users 

Check to see if the current user is following one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_if_user_follows_playlist

> Vec<bool> check_if_user_follows_playlist(playlist_id, ids)
Check if Current User Follows Playlist 

Check to see if the current user is following a specified playlist.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**ids** | Option<**String**> |  |  |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## follow_artists_users

> follow_artists_users(r#type, ids, follow_artists_users_request)
Follow Artists or Users 

Add the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**follow_artists_users_request** | Option<[**FollowArtistsUsersRequest**](FollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## follow_playlist

> follow_playlist(playlist_id, follow_playlist_request)
Follow Playlist 

Add the current user as a follower of a playlist.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**follow_playlist_request** | Option<[**FollowPlaylistRequest**](FollowPlaylistRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_current_users_profile

> models::PrivateUserObject get_current_users_profile()
Get Current User's Profile 

Get detailed profile information about the current user (including the current user's username). 

### Parameters

This endpoint does not need any parameter.

### Return type

[**models::PrivateUserObject**](PrivateUserObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_followed

> models::GetFollowed200Response get_followed(r#type, after, limit)
Get Followed Artists 

Get the current user's followed artists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**after** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]

### Return type

[**models::GetFollowed200Response**](get_followed_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_list_users_playlists

> models::PagingPlaylistObject get_list_users_playlists(user_id, limit, offset)
Get User's Playlists 

Get a list of the playlists owned or followed by a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**user_id** | **String** |  | [required] |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingPlaylistObject**](PagingPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_profile

> models::PublicUserObject get_users_profile(user_id)
Get User's Profile 

Get public profile information about a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**user_id** | **String** |  | [required] |

### Return type

[**models::PublicUserObject**](PublicUserObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_top_artists_and_tracks

> models::GetUsersTopArtistsAndTracks200Response get_users_top_artists_and_tracks(r#type, time_range, limit, offset)
Get User's Top Items 

Get the current user's top artists or tracks based on calculated affinity. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**time_range** | Option<**String**> |  |  |[default to medium_term]
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::GetUsersTopArtistsAndTracks200Response**](get_users_top_artists_and_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## unfollow_artists_users

> unfollow_artists_users(r#type, ids, unfollow_artists_users_request)
Unfollow Artists or Users 

Remove the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**unfollow_artists_users_request** | Option<[**UnfollowArtistsUsersRequest**](UnfollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## unfollow_playlist

> unfollow_playlist(playlist_id)
Unfollow Playlist 

Remove the current user as a follower of a playlist.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

